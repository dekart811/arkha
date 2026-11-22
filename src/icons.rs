//! Icon lookup with caching, and a background thumbnailer that writes to the
//! shared freedesktop thumbnail cache (~/.cache/thumbnails), same as Caja.
//! Images are thumbnailed built in; other types by custom thumbnailers.

use crate::thumbnailers::{self, Thumbnailer};

use gdk_pixbuf::Pixbuf;
use gio::prelude::*;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

thread_local! {
    /// Theme icons by their GIcon's text and size.
    static ICONS: RefCell<HashMap<String, HashMap<i32, Option<Pixbuf>>>> = RefCell::new(HashMap::new());
    /// Loaded thumbnails by path and size.
    static THUMBS: RefCell<HashMap<String, HashMap<i32, Option<Pixbuf>>>> = RefCell::new(HashMap::new());
    /// Thumbnails with their shadow (see framed_thumb), and their sizes.
    static FRAMED: RefCell<HashMap<(String, i32), Pixbuf>> = RefCell::new(HashMap::new());
    /// The memory the pictures in THUMBS and FRAMED take (see CACHED_BYTES).
    static THUMB_BYTES: Cell<usize> = const { Cell::new(0) };
    static FRAMED_BYTES: Cell<usize> = const { Cell::new(0) };
    static DIMS: RefCell<HashMap<String, Option<ThumbHeader>>> = RefCell::new(HashMap::new());
    /// Thumbnails being loaded, and the loaders' queue (see thumb_pixbuf).
    static LOADING: RefCell<HashSet<(String, i32)>> = RefCell::new(HashSet::new());
    static LOAD_QUEUE: RefCell<Option<Arc<Pool<ThumbKey>>>> = const { RefCell::new(None) };
    /// Views being drawn (see set_drawing).
    static DRAWING: Cell<u32> = const { Cell::new(0) };
    /// (uri, mtime, size) sent to the makers and not done yet, so a file
    /// isn't queued twice; an edited file gets a new entry, as does one
    /// that grew within the same second (it was still arriving when it was
    /// tried). Done, the thumbnail or the failure noted takes over: the
    /// entry goes, so this doesn't grow as long as Arkha runs.
    static QUEUED: RefCell<HashSet<(String, i64, u64)>> = RefCell::new(HashSet::new());
    static MAKERS: RefCell<Option<Arc<Pool<ThumbJob>>>> = const { RefCell::new(None) };
    /// The custom thumbnailer for each content type asked about.
    /// Shared, not copied: asked about for every file of a folder.
    static CUSTOM_FOR: RefCell<HashMap<String, Option<Arc<Thumbnailer>>>> = RefCell::new(HashMap::new());
    static LISTENERS: RefCell<Vec<(usize, Listener)>> = RefCell::new(Vec::new());
    static NEXT_LISTENER: Cell<usize> = const { Cell::new(0) };
    /// The custom thumbnailers whose programs are installed, loaded on
    /// first use (see reload_thumbnailers).
    static CUSTOM: RefCell<Option<Vec<Thumbnailer>>> = const { RefCell::new(None) };
    /// The system's thumbnailers (see thumbnailers::system), loaded on
    /// first use.
    static SYSTEM: RefCell<Option<Vec<Thumbnailer>>> = const { RefCell::new(None) };
}

/// The thumbnailer for `ctype`: the user's (custom) first, then for what
/// isn't an image (those are made here) one of the system's.
fn custom_for(ctype: &str) -> Option<Arc<Thumbnailer>> {
    if let Some(t) = CUSTOM_FOR.with(|c| c.borrow().get(ctype).cloned()) {
        return t;
    }
    let t = CUSTOM.with(|c| {
        let mut c = c.borrow_mut();
        let list = c.get_or_insert_with(|| thumbnailers::load().into_iter().filter(thumbnailers::installed).collect());
        thumbnailers::find(list, ctype).map(Arc::new)
    });
    let t = t.or_else(|| {
        if builtin_can(ctype) {
            return None;
        }
        SYSTEM.with(|c| {
            let mut c = c.borrow_mut();
            let list = c.get_or_insert_with(thumbnailers::system);
            thumbnailers::find(list, ctype).map(Arc::new)
        })
    });
    CUSTOM_FOR.with(|c| c.borrow_mut().insert(ctype.to_string(), t.clone()));
    t
}

/// Re-reads the custom thumbnailers and lets files be tried again.
pub fn reload_thumbnailers() {
    CUSTOM.with(|c| *c.borrow_mut() = None);
    SYSTEM.with(|c| *c.borrow_mut() = None);
    CUSTOM_FOR.with(|c| c.borrow_mut().clear());
    QUEUED.with(|q| q.borrow_mut().clear());
    // Files a thumbnailer failed on may work with the new ones.
    let _ = std::fs::remove_dir_all(fail_dir());
}

thread_local! {
    /// Changes when ICONS is emptied: the pictures kept on icons (see
    /// icon_pixbuf) are then out of date.
    static ICONS_GEN: Cell<u32> = const { Cell::new(0) };
    static PICTURES_KEY: glib::Quark = glib::Quark::from_str("arkha-icon-pictures");
}

/// Pictures kept on an icon: (ICONS_GEN, size, picture).
type KeptPictures = RefCell<Vec<(u32, i32, Option<Pixbuf>)>>;

/// A GIcon's text, which names it in the cache. Making it takes long for
/// what views do for every item each time they measure them, so it's
/// kept on the icon.
fn icon_key(icon: &gio::Icon) -> String {
    let quark = glib::Quark::from_str("arkha-icon-key");
    unsafe {
        if let Some(k) = icon.qdata::<String>(quark) {
            return k.as_ref().clone();
        }
        let k = IconExt::to_string(icon).map(|s| s.to_string()).unwrap_or_else(|| "text-x-generic".into());
        icon.set_qdata(quark, k.clone());
        k
    }
}

// ---------------------------------------------------------------------------
// Text in icons (Preferences → Previews, as in Caja)

/// A text file's first lines, as read: by URI, with the modification time
/// read at; None while being read, "" for a file that isn't text after all.
type Texts = HashMap<String, (i64, Option<Rc<str>>)>;

thread_local! {
    static TEXTS: RefCell<Texts> = RefCell::new(HashMap::new());
    /// Icons with their file's text, by (URI, size, modification time,
    /// icon: its emblems may change).
    static TEXT_ICONS: RefCell<HashMap<(String, i32, i64, String), Pixbuf>> = RefCell::new(HashMap::new());
    /// Whether a content type is text, as asked.
    static IS_TEXT: RefCell<HashMap<String, bool>> = RefCell::new(HashMap::new());
    static TEXT_READER: RefCell<Option<async_channel::Sender<(String, std::path::PathBuf, i64)>>> = const { RefCell::new(None) };
    /// What lays the text out (any widget's Pango context does).
    static LAYOUTS: gtk::Label = gtk::Label::new(None);
}

/// Whether files of `ctype` show their text in their icon: plain text and
/// its kinds (source code, scripts, …), as in Caja.
pub fn is_text(ctype: &str) -> bool {
    IS_TEXT.with(|c| *c.borrow_mut().entry(ctype.to_string()).or_insert_with(|| gio::content_type_is_a(ctype, "text/plain")))
}

/// The start of a text file for its icon: the first lines, tabs as spaces;
/// "" if it isn't text (a NUL byte in the first 4 KB).
pub fn read_text_start(path: &std::path::Path) -> String {
    use std::io::Read;
    let mut buf = vec![0u8; 4096];
    let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut buf)).unwrap_or(0);
    buf.truncate(n);
    if buf.contains(&0) {
        return String::new();
    }
    String::from_utf8_lossy(&buf)
        .lines()
        .take(40)
        .map(|l| l.replace('\t', "    ").chars().filter(|c| !c.is_control()).take(100).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Reads text files' starts in a thread of its own; each read is told to
/// the thumbnail listeners with no thumbnail ("") so the views redraw.
fn text_reader() -> async_channel::Sender<(String, std::path::PathBuf, i64)> {
    if let Some(tx) = TEXT_READER.with(|r| r.borrow().clone()) {
        return tx;
    }
    let (tx, rx) = async_channel::unbounded::<(String, std::path::PathBuf, i64)>();
    let (done_tx, done_rx) = async_channel::unbounded::<(String, i64, String)>();
    std::thread::spawn(move || {
        while let Ok((uri, path, mtime)) = rx.recv_blocking() {
            if done_tx.send_blocking((uri, mtime, read_text_start(&path))).is_err() {
                return;
            }
        }
    });
    glib::MainContext::default().spawn_local(async move {
        while let Ok((uri, mtime, text)) = done_rx.recv().await {
            TEXTS.with(|t| {
                let mut t = t.borrow_mut();
                // Kept within bounds: a big folder's worth.
                if t.len() > 5000 {
                    t.clear();
                }
                t.insert(uri.clone(), (mtime, Some(Rc::from(text))));
            });
            LISTENERS.with(|l| {
                for (_, f) in l.borrow().iter() {
                    f(&uri, "");
                }
            });
        }
    });
    TEXT_READER.with(|r| *r.borrow_mut() = Some(tx.clone()));
    tx
}

/// A text file's `icon` at `size` pixels with the file's first lines on
/// it, as Caja shows text files, its emblems over them; None until they're
/// read (they're asked for here) or if it isn't text.
pub fn text_icon(uri: &str, path: &std::path::Path, mtime: i64, size: i32, icon: &gio::Icon) -> Option<Pixbuf> {
    let key = (uri.to_string(), size, mtime, icon_key(icon));
    if let Some(p) = TEXT_ICONS.with(|c| c.borrow().get(&key).cloned()) {
        return Some(p);
    }
    let known = TEXTS.with(|t| t.borrow().get(uri).cloned());
    let text = match known {
        Some((m, Some(text))) if m == mtime => text,
        // Being read.
        Some((m, None)) if m == mtime => return None,
        _ => {
            TEXTS.with(|t| t.borrow_mut().insert(uri.to_string(), (mtime, None)));
            let _ = text_reader().send_blocking((uri.to_string(), path.to_path_buf(), mtime));
            return None;
        }
    };
    if text.is_empty() {
        return None;
    }
    // On the icon without its emblems, which then go on top (as GTK puts
    // them on any icon).
    let em = icon.downcast_ref::<gio::EmblemedIcon>();
    let plain = em.map_or_else(|| icon.clone(), |e| e.icon());
    let page = draw_text_on(&icon_pixbuf(Some(&plain), size)?, &text, size)?;
    let pb = match em {
        Some(e) if !e.emblems().is_empty() => {
            let with = gio::EmblemedIcon::new(&page, None::<&gio::Emblem>);
            for emblem in e.emblems() {
                with.add_emblem(&emblem);
            }
            gtk::IconTheme::default()
                .and_then(|t| t.lookup_by_gicon(&with, size, gtk::IconLookupFlags::FORCE_SIZE))
                .and_then(|i| i.load_icon().ok())
                .unwrap_or(page)
        }
        _ => page,
    };
    TEXT_ICONS.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 400 {
            c.clear();
        }
        c.insert(key, pb.clone());
    });
    Some(pb)
}

/// A document icon's page colour (0–1 each): at an empty spot near its
/// top, left of where themes put a symbol; white if it can't be told.
fn page_colour(base: &Pixbuf) -> (f64, f64, f64) {
    let (x, y) = ((base.width() as f64 * 0.3) as usize, (base.height() as f64 * 0.27) as usize);
    let n = base.n_channels() as usize;
    let bytes = base.read_pixel_bytes();
    let at = y * base.rowstride() as usize + x * n;
    match bytes.get(at..at + n) {
        Some(p) if n < 4 || p[3] > 128 => (p[0] as f64 / 255.0, p[1] as f64 / 255.0, p[2] as f64 / 255.0),
        _ => (1.0, 1.0, 1.0),
    }
}

/// Draws `text` small, in the page of a document icon.
fn draw_text_on(base: &Pixbuf, text: &str, size: i32) -> Option<Pixbuf> {
    let (w, h) = (base.width(), base.height());
    let surface = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w, h).ok()?;
    {
        use gdk::prelude::GdkContextExt;
        let cr = gtk::cairo::Context::new(&surface).ok()?;
        cr.set_source_pixbuf(base, 0.0, 0.0);
        cr.paint().ok()?;
        // Where a document icon's page is (themes don't say any more), made
        // blank in the page's own colour (taken from an empty spot of it):
        // themes draw lines or a symbol there.
        let (x0, y0, x1, y1) = (w as f64 * 0.24, h as f64 * 0.34, w as f64 * 0.76, h as f64 * 0.86);
        let (r, g, b) = page_colour(base);
        cr.rectangle(x0, y0, x1 - x0, y1 - y0);
        cr.set_source_rgb(r, g, b);
        let _ = cr.fill_preserve();
        cr.clip();
        let pad = (size as f64 / 48.0).max(1.0);
        LAYOUTS.with(|l| {
            let layout = l.create_pango_layout(Some(text));
            let mut font = pango::FontDescription::from_string("Monospace");
            font.set_absolute_size((size as f64 / 16.0).max(2.0) * pango::SCALE as f64);
            layout.set_font_description(Some(&font));
            // Dark on a light page, light on a dark one, whatever the theme's
            // text colour.
            let attrs = pango::AttrList::new();
            let ink: u16 = if 0.299 * r + 0.587 * g + 0.114 * b > 0.5 { 0x3000 } else { 0xe000 };
            attrs.insert(pango::AttrColor::new_foreground(ink, ink, ink));
            attrs.insert(pango::AttrInt::new_foreground_alpha(0xd000));
            layout.set_attributes(Some(&attrs));
            gtk::render_layout(&l.style_context(), &cr, x0 + pad, y0 + pad, &layout);
        });
    }
    surface.flush();
    gdk::pixbuf_get_from_surface(&surface, 0, 0, w, h)
}

/// The emblems the user can't choose (Caja adds them by itself).
const AUTOMATIC_EMBLEMS: &[&str] = &["emblem-trash", "emblem-symbolic-link", "emblem-noread", "emblem-nowrite", "emblem-note", "emblem-shared", "emblem-unreadable"];

/// The emblems of the icon theme a user can give files, as Caja offers
/// them: (icon name, name to show), in that order.
pub fn available_emblems() -> Vec<(String, String)> {
    let Some(theme) = gtk::IconTheme::default() else { return vec![] };
    let mut out: Vec<(String, String)> = theme
        .list_icons(Some("Emblems"))
        .into_iter()
        .map(|n| n.to_string())
        .filter(|n| n.starts_with("emblem-") && !n.ends_with("-symbolic") && !AUTOMATIC_EMBLEMS.contains(&n.as_str()))
        .map(|n| {
            let name = emblem_title(&n);
            (n, name)
        })
        .collect();
    out.sort_by_key(|a| a.1.to_lowercase());
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// "emblem-important" → "Important".
pub fn emblem_title(icon: &str) -> String {
    let words = icon.strip_prefix("emblem-").unwrap_or(icon).replace(['-', '_'], " ");
    let mut c = words.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Marks icons made from a custom icon (see item_icon): no thumbnail
/// replaces them, as in Caja.
fn custom_quark() -> glib::Quark {
    glib::Quark::from_str("arkha-custom-icon")
}

pub fn is_custom(icon: &gio::Icon) -> bool {
    // SAFETY: only ever set (in item_icon) with this type.
    unsafe { icon.qdata::<bool>(custom_quark()).is_some() }
}

/// A file's icon as the views show it: its custom icon if it has one
/// (metadata::custom-icon, a picture; or metadata::custom-icon-name), else
/// its own, with the emblems the user gave it (metadata::emblems, Caja's
/// keywords: "important" for emblem-important), a note's emblem if it has
/// one, and `automatic` ones (a link's, say) first.
pub fn item_icon(info: &gio::FileInfo, automatic: &[&str]) -> Option<gio::Icon> {
    let custom: Option<gio::Icon> = info
        .attribute_string("metadata::custom-icon")
        .filter(|u| !u.is_empty())
        .map(|u| gio::FileIcon::new(&if u.contains("://") { gio::File::for_uri(&u) } else { gio::File::for_path(u.as_str()) }).upcast())
        .or_else(|| info.attribute_string("metadata::custom-icon-name").filter(|n| !n.is_empty()).map(|n| gio::ThemedIcon::new(&n).upcast()));
    let is_custom = custom.is_some();
    let base = custom.or_else(|| info.icon())?;
    let mut emblems: Vec<String> = automatic.iter().map(|e| e.to_string()).collect();
    if info.attribute_string("metadata::annotation").is_some_and(|n| !n.is_empty()) {
        emblems.push("emblem-note".into());
    }
    if info.has_attribute("metadata::emblems") {
        emblems.extend(info.attribute_stringv("metadata::emblems").iter().filter(|k| !k.is_empty()).map(|k| format!("emblem-{k}")));
    }
    let icon = if emblems.is_empty() {
        base
    } else {
        let em = gio::EmblemedIcon::new(&base, None::<&gio::Emblem>);
        for e in &emblems {
            em.add_emblem(&gio::Emblem::new(&gio::ThemedIcon::new(e)));
        }
        em.upcast()
    };
    if is_custom {
        // SAFETY: read back as this type only (is_custom).
        unsafe { icon.set_qdata(custom_quark(), true) };
    }
    Some(icon)
}

pub fn icon_pixbuf(icon: Option<&gio::Icon>, size: i32) -> Option<Pixbuf> {
    // Asked for every item drawn, at every step of scrolling: kept on the
    // icon too, which saves naming it and two lookups.
    let generation = ICONS_GEN.with(|g| g.get());
    let quark = PICTURES_KEY.with(|q| *q);
    if let Some(i) = icon {
        // SAFETY: only ever set below, with this type.
        if let Some(kept) = unsafe { i.qdata::<KeptPictures>(quark) } {
            let kept = unsafe { kept.as_ref() }.borrow();
            if let Some((_, _, p)) = kept.iter().find(|(g, s, _)| *g == generation && *s == size) {
                return p.clone();
            }
        }
    }
    let pb = icon_pixbuf_cached(icon, size);
    if let Some(i) = icon {
        unsafe {
            match i.qdata::<KeptPictures>(quark) {
                Some(kept) => {
                    let mut kept = kept.as_ref().borrow_mut();
                    kept.retain(|(g, s, _)| *g == generation && *s != size);
                    kept.push((generation, size, pb.clone()));
                }
                None => i.set_qdata::<KeptPictures>(quark, RefCell::new(vec![(generation, size, pb.clone())])),
            }
        }
    }
    pb
}

fn icon_pixbuf_cached(icon: Option<&gio::Icon>, size: i32) -> Option<Pixbuf> {
    let key = icon.map_or_else(|| "text-x-generic".to_string(), icon_key);
    if let Some(p) = ICONS.with(|c| c.borrow().get(&key).and_then(|s| s.get(&size).cloned())) {
        return p;
    }
    let theme = gtk::IconTheme::default()?;
    let flags = gtk::IconLookupFlags::FORCE_SIZE;
    let pb = icon
        .and_then(|i| theme.lookup_by_gicon(i, size, flags))
        .and_then(|info| info.load_icon().ok())
        .or_else(|| {
            theme
                .load_icon("text-x-generic", size, flags)
                .ok()
                .flatten()
        })
        .or_else(|| {
            theme
                .load_icon("application-x-generic", size, flags)
                .ok()
                .flatten()
        });
    ICONS.with(|c| c.borrow_mut().entry(key).or_default().insert(size, pb.clone()));
    pb
}

pub fn named_pixbuf(name: &str, size: i32) -> Option<Pixbuf> {
    let icon: gio::Icon = gio::ThemedIcon::new(name).upcast();
    icon_pixbuf(Some(&icon), size)
}

/// Load a thumbnail file, scaled so its longest side is `size`.
/// A thumbnail file's picture, scaled so its longest side is `size`, if
/// it's loaded. Decoding takes milliseconds per picture (seconds for a big
/// folder), so it's done in the background: while a view is being drawn,
/// a missing one is queued, and the windows are redrawn once it's loaded.
/// Views measure all their items, drawn or not, so that's what keeps
/// pictures nobody sees from being loaded.
pub fn thumb_pixbuf(path: &str, size: i32) -> Option<Pixbuf> {
    if let Some(p) = THUMBS.with(|c| c.borrow().get(path).and_then(|s| s.get(&size).cloned())) {
        return p;
    }
    if DRAWING.with(|d| d.get()) > 0 {
        load_thumb(path, size);
    }
    None
}

/// Views call this before (true) and after (false) drawing their items.
/// Whether a view is being drawn (see set_drawing).
pub fn drawing() -> bool {
    DRAWING.with(|d| d.get()) > 0
}

pub fn set_drawing(on: bool) {
    DRAWING.with(|d| d.set(if on { d.get() + 1 } else { d.get().saturating_sub(1) }));
}

/// A thumbnail to load: (path, size).
type ThumbKey = (String, i32);

/// Jobs for threads of their own, the latest first: views ask for what
/// they show, so that's what was just scrolled to.
struct Pool<J> {
    jobs: Mutex<Vec<J>>,
    ready: Condvar,
}

impl<J: Send + 'static> Pool<J> {
    /// `threads` threads, each doing `work` for one job after another and
    /// sending what it gives on `tx` (they end once nothing receives it).
    fn start<R: Send + 'static>(threads: usize, work: impl Fn(J) -> R + Send + Sync + 'static, tx: async_channel::Sender<R>) -> Arc<Pool<J>> {
        let pool = Arc::new(Pool { jobs: Mutex::new(vec![]), ready: Condvar::new() });
        let work = Arc::new(work);
        for _ in 0..threads {
            let (pool, work, tx) = (pool.clone(), work.clone(), tx.clone());
            std::thread::spawn(move || loop {
                let job = {
                    let mut jobs = pool.jobs.lock().unwrap();
                    loop {
                        if let Some(j) = jobs.pop() {
                            break j;
                        }
                        jobs = pool.ready.wait(jobs).unwrap();
                    }
                };
                if tx.send_blocking(work(job)).is_err() {
                    return;
                }
            });
        }
        pool
    }

    fn push(&self, job: J) {
        self.jobs.lock().unwrap().push(job);
        self.ready.notify_one();
    }
}

/// Loads a thumbnail (path, size) on the loaders' threads: the latest asked
/// for first, those are the ones shown now.
fn load_thumb(path: &str, size: i32) {
    let key = (path.to_string(), size);
    if !LOADING.with(|l| l.borrow_mut().insert(key.clone())) {
        return;
    }
    LOAD_QUEUE.with(|q| q.borrow_mut().get_or_insert_with(start_loaders).clone()).push(key);
}

fn start_loaders() -> Arc<Pool<ThumbKey>> {
    // A Pixbuf can't be sent between threads, its pixels can.
    type Pixels = (glib::Bytes, bool, i32, i32, i32);
    let (tx, rx) = async_channel::unbounded::<(String, i32, Option<Pixels>)>();
    let q = Pool::start(
        4,
        |(path, size): ThumbKey| {
            let pixels = load_at_scale(std::path::Path::new(&path), size)
                .ok()
                .map(|p| (p.read_pixel_bytes(), p.has_alpha(), p.width(), p.height(), p.rowstride()));
            (path, size, pixels)
        },
        tx,
    );
    glib::MainContext::default().spawn_local(async move {
        while let Ok(first) = rx.recv().await {
            // Everything loaded meanwhile, then one redraw.
            let mut done = vec![first];
            while let Ok(more) = rx.try_recv() {
                done.push(more);
            }
            THUMBS.with(|c| {
                let mut c = c.borrow_mut();
                LOADING.with(|l| {
                    let mut l = l.borrow_mut();
                    for (path, size, pixels) in done {
                        l.remove(&(path.clone(), size));
                        let pb = pixels.map(|(bytes, alpha, w, h, stride)| {
                            Pixbuf::from_bytes(&bytes, gdk_pixbuf::Colorspace::Rgb, alpha, 8, w, h, stride)
                        });
                        if make_room(&THUMB_BYTES, c.len(), pb.as_ref().map_or(0, pixbuf_bytes)) {
                            c.clear();
                        }
                        if let Some(Some(old)) = c.entry(path).or_default().insert(size, pb) {
                            THUMB_BYTES.with(|b| b.set(b.get().saturating_sub(pixbuf_bytes(&old))));
                        }
                    }
                });
            });
            for w in gtk::Window::list_toplevels() {
                w.queue_draw();
            }
        }
    });
    q
}

/// The most memory loaded thumbnails may take, and so again for those
/// with their shadow: past it they're let go (and loaded again as they're
/// drawn). Many windows full even at the largest size, while a folder of
/// thousands of pictures, scrolled through, would otherwise keep them all.
const CACHED_BYTES: usize = 96 << 20;

fn pixbuf_bytes(p: &Pixbuf) -> usize {
    p.rowstride().max(0) as usize * p.height().max(0) as usize
}

/// Counts a picture of `add` bytes about to go into a cache of `len`
/// entries; true if the cache is to be emptied first (it would hold too
/// much), the count then starting again.
fn make_room(count: &'static std::thread::LocalKey<Cell<usize>>, len: usize, add: usize) -> bool {
    count.with(|c| {
        let full = len >= 4000 || c.get() + add > CACHED_BYTES;
        c.set(if full { add } else { c.get() + add });
        full
    })
}

pub fn forget_thumb(path: &str) {
    let gone = THUMBS.with(|c| c.borrow_mut().remove(path)).map_or(0, |sizes| sizes.values().flatten().map(pixbuf_bytes).sum());
    THUMB_BYTES.with(|c| c.set(c.get().saturating_sub(gone)));
    FRAMED.with(|c| {
        let mut gone = 0;
        c.borrow_mut().retain(|k, p| {
            let keep = k.0 != path;
            if !keep {
                gone += pixbuf_bytes(p);
            }
            keep
        });
        FRAMED_BYTES.with(|c| c.set(c.get().saturating_sub(gone)));
    });
    DIMS.with(|c| c.borrow_mut().remove(path));
}

/// What a thumbnail file's PNG header tells before it's decoded: a view
/// can lay out its item before the picture is loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThumbHeader {
    pub width: i32,
    pub height: i32,
    /// Whether it has transparency (an alpha channel, or a tRNS chunk).
    pub alpha: bool,
}

pub fn thumb_header(path: &str) -> Option<ThumbHeader> {
    if let Some(d) = DIMS.with(|c| c.borrow().get(path).copied()) {
        return d;
    }
    let d = (|| {
        use std::io::Read;
        // The chunks before the picture's data are in its first few
        // kilobytes (thumbnails' text chunks are short).
        let mut head = Vec::with_capacity(8192);
        std::fs::File::open(path).ok()?.take(8192).read_to_end(&mut head).ok()?;
        parse_png_header(&head)
    })();
    DIMS.with(|c| c.borrow_mut().insert(path.to_string(), d));
    d
}

fn parse_png_header(b: &[u8]) -> Option<ThumbHeader> {
    if b.len() < 26 || &b[..8] != b"\x89PNG\r\n\x1a\n" || &b[12..16] != b"IHDR" {
        return None;
    }
    let be = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let (width, height) = (be(16) as i32, be(20) as i32);
    // Colour types 4 and 6 have an alpha channel; others may have a tRNS
    // chunk, which comes before the picture's data.
    let mut alpha = matches!(b[25], 4 | 6);
    let mut i = 8;
    while !alpha && i + 8 <= b.len() {
        let (len, kind) = (be(i) as usize, &b[i + 4..i + 8]);
        if kind == b"IDAT" {
            break;
        }
        alpha = kind == b"tRNS";
        i += 12 + len;
    }
    (width > 0 && height > 0).then_some(ThumbHeader { width, height, alpha })
}

/// A picture `w`×`h` scaled so its longest side is `size`.
pub fn fit(w: i32, h: i32, size: i32) -> (i32, i32) {
    let scale = size as f64 / w.max(h) as f64;
    (((w as f64 * scale).round() as i32).max(1), ((h as f64 * scale).round() as i32).max(1))
}

/// How far a thumbnail's shadow reaches (see framed_thumb).
pub const SHADOW: i32 = 3;

/// A thumbnail as Caja shows it in the icon view, with a shadow below and
/// to the right: `SHADOW` pixels more at the bottom and on both sides (so
/// the picture stays centered). None while it's still being loaded. Caja
/// leaves it out for pictures with transparency (see pane's
/// icon_view_thumb): it would show through.
pub fn framed_thumb(path: &str, size: i32) -> Option<Pixbuf> {
    let key = (path.to_string(), size);
    if let Some(p) = FRAMED.with(|c| c.borrow().get(&key).cloned()) {
        return Some(p);
    }
    let pb = thumb_pixbuf(path, size)?;
    let (w, h) = (pb.width(), pb.height());
    let surface = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w + 2 * SHADOW, h + SHADOW).ok()?;
    {
        use gdk::prelude::GdkContextExt;
        let cr = gtk::cairo::Context::new(&surface).ok()?;
        // Copies offset by 1 to 3 pixels: darker close to the picture.
        for d in 1..=SHADOW {
            cr.rectangle((SHADOW + d) as f64, d as f64, w as f64, h as f64);
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.3);
            let _ = cr.fill();
        }
        // On white, as Caja frames them: a document's preview may leave its
        // page transparent (only the text drawn), which would show the
        // shadow through. (Transparent pictures aren't framed at all.)
        cr.rectangle(SHADOW as f64, 0.0, w as f64, h as f64);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        let _ = cr.fill();
        cr.set_source_pixbuf(&pb, SHADOW as f64, 0.0);
        let _ = cr.paint();
    }
    let framed = gdk::pixbuf_get_from_surface(&surface, 0, 0, w + 2 * SHADOW, h + SHADOW)?;
    FRAMED.with(|c| {
        let mut c = c.borrow_mut();
        if make_room(&FRAMED_BYTES, c.len(), pixbuf_bytes(&framed)) {
            c.clear();
        }
        if let Some(old) = c.insert(key, framed.clone()) {
            FRAMED_BYTES.with(|b| b.set(b.get().saturating_sub(pixbuf_bytes(&old))));
        }
    });
    Some(framed)
}

struct ThumbJob {
    uri: String,
    src: std::path::PathBuf,
    mtime: i64,
    /// The file's size in bytes, noted with a failure (see record_failure).
    size: u64,
    out: std::path::PathBuf,
    /// A custom thumbnailer to make the picture; None for built-in images.
    custom: Option<Arc<Thumbnailer>>,
}

pub fn thumb_path_for(uri: &str) -> std::path::PathBuf {
    let md5 = glib::compute_checksum_for_data(glib::ChecksumType::Md5, uri.as_bytes())
        .map(|s| s.to_string())
        .unwrap_or_default();
    glib::user_cache_dir()
        .join("thumbnails")
        .join("large")
        .join(format!("{md5}.png"))
}

/// Register a callback fired on the main thread when a thumbnail is ready: (uri, thumb_path).
/// Returns an id for remove_thumbnail_listener.
pub fn on_thumbnail_ready(f: impl Fn(&str, &str) + 'static) -> usize {
    let id = NEXT_LISTENER.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    LISTENERS.with(|l| l.borrow_mut().push((id, Box::new(f))));
    id
}

pub fn remove_thumbnail_listener(id: usize) {
    LISTENERS.with(|l| l.borrow_mut().retain(|(i, _)| *i != id));
}

/// Called with (uri, thumbnail path) when a thumbnail is made.
type Listener = Box<dyn Fn(&str, &str)>;

/// How many thumbnails are made at once (see makers_for); 1 if the number
/// of processors can't be told.
fn maker_count() -> usize {
    std::thread::available_parallelism().map_or(1, |n| makers_for(n.get()))
}

/// Thumbnail makers for `cpus` processors: one fewer than there are, so
/// one is always left for the rest (the window drawing, other programs),
/// at most 8, and at least 1.
fn makers_for(cpus: usize) -> usize {
    if cpus > 8 {
        8
    } else {
        cpus.saturating_sub(1).max(1)
    }
}

/// Loads the theme icons the images in `root` are about to show into GTK's
/// own cache, several at once (as many as thumbnails are made at once) on
/// GLib's threads, and waits for them, at most a second. Left to GTK, a new
/// window would load them one by one on the main thread when shown, which
/// takes about 3 ms each with gdk-pixbuf's sandboxed (glycin) loaders,
/// some 200 ms for a window. Called once the widgets are shown and the
/// window isn't yet: only visible images count. Each icon is looked up as
/// GtkImage does (name, size, flags), a symbolic one in its widget's
/// colours, so that GTK then finds it loaded; one it looks up differently
/// is simply loaded again.
pub fn preload_widget_icons(root: &gtk::Widget) {
    let Some(theme) = gtk::IconTheme::default() else { return };
    let mut jobs = Vec::new();
    collect_widget_icons(root, &theme, &mut jobs, &mut HashSet::new());
    if jobs.is_empty() {
        return;
    }
    // The loads report back to a context of their own, so that nothing
    // else runs while waiting.
    let ctx = glib::MainContext::new();
    let _ = ctx.with_thread_default(|| {
        let pending = Rc::new(Cell::new(jobs.len()));
        let queue = Rc::new(RefCell::new(jobs.into_iter()));
        let late = Arc::new(AtomicBool::new(false));
        let timer = {
            let late = late.clone();
            glib::timeout_source_new(std::time::Duration::from_secs(1), None, glib::Priority::DEFAULT, move || {
                late.store(true, Ordering::Relaxed);
                glib::ControlFlow::Break
            })
        };
        timer.attach(Some(&ctx));
        for _ in 0..maker_count() {
            preload_next(&queue, &pending);
        }
        while pending.get() > 0 && !late.load(Ordering::Relaxed) {
            ctx.iteration(true);
        }
        timer.destroy();
    });
}

/// An icon to load: its lookup, and for a symbolic one its widget's style.
type IconJob = (gtk::IconInfo, Option<gtk::StyleContext>);

fn preload_next(queue: &Rc<RefCell<std::vec::IntoIter<IconJob>>>, pending: &Rc<Cell<usize>>) {
    let Some((info, style)) = queue.borrow_mut().next() else { return };
    let (queue, pending) = (queue.clone(), pending.clone());
    let done = move || {
        pending.set(pending.get() - 1);
        preload_next(&queue, &pending);
    };
    match style {
        Some(style) => info.load_symbolic_for_context_async(&style, gio::Cancellable::NONE, move |_| done()),
        None => info.load_icon_async(gio::Cancellable::NONE, move |_| done()),
    }
}

/// The icons of the visible images in `w`, each lookup (and colour) once.
fn collect_widget_icons(w: &gtk::Widget, theme: &gtk::IconTheme, jobs: &mut Vec<IconJob>, seen: &mut HashSet<(usize, String)>) {
    if let Some((info, style)) = w.downcast_ref::<gtk::Image>().and_then(|i| image_icon(i, theme)) {
        let colour = style.as_ref().map(|s| s.color(s.state()).to_string()).unwrap_or_default();
        if seen.insert((info.as_ptr() as usize, colour)) {
            jobs.push((info, style));
        }
    }
    if let Some(c) = w.downcast_ref::<gtk::Container>() {
        // Its own flag: is_visible() would also want the window shown.
        c.forall(|child| {
            if child.get_visible() {
                collect_widget_icons(child, theme, jobs, seen);
            }
        });
    }
}

/// The icon `img` shows from the theme, looked up as GTK 3's GtkIconHelper
/// does, with the style to colour it in if it's symbolic.
fn image_icon(img: &gtk::Image, theme: &gtk::IconTheme) -> Option<IconJob> {
    let icon: gio::Icon = match img.storage_type() {
        gtk::ImageType::IconName => {
            let name = img.icon_name()?;
            if img.uses_fallback() {
                gio::ThemedIcon::with_default_fallbacks(&name).upcast()
            } else {
                gio::ThemedIcon::new(&name).upcast()
            }
        }
        gtk::ImageType::Gicon => img.gicon().0,
        _ => return None,
    };
    let size = match img.pixel_size() {
        n if n > 0 => n,
        _ => img.icon_size().lookup().map(|(w, h)| w.min(h))?,
    };
    let style = img.style_context();
    let mut flags = gtk::IconLookupFlags::USE_BUILTIN;
    if img.uses_fallback() {
        flags |= gtk::IconLookupFlags::GENERIC_FALLBACK;
    }
    match img.direction() {
        gtk::TextDirection::Ltr => flags |= gtk::IconLookupFlags::DIR_LTR,
        gtk::TextDirection::Rtl => flags |= gtk::IconLookupFlags::DIR_RTL,
        _ => {}
    }
    // A theme's -gtk-icon-style (forcing regular or symbolic icons) can't
    // be read; with one, GTK looks them up differently and loads them itself.
    let info = theme.lookup_by_gicon_for_scale(&icon, size, img.scale_factor(), flags)?;
    let symbolic = info.is_symbolic();
    Some((info, symbolic.then_some(style)))
}

/// The thumbnails to make, the latest asked for first.
fn makers() -> Arc<Pool<ThumbJob>> {
    if let Some(q) = MAKERS.with(|m| m.borrow().clone()) {
        return q;
    }
    let (done_tx, done_rx) = async_channel::unbounded::<(String, i64, u64, Option<String>)>();
    let q = Pool::start(
        maker_count(),
        |job: ThumbJob| {
            // Gone since it was asked for (renamed, moved or deleted while
            // its row was still shown): nothing to make, and no failure to
            // note.
            if !job.src.exists() {
                glib::g_debug!("arkha", "thumbnail: {} is gone", job.uri);
                return (job.uri, job.mtime, job.size, None);
            }
            let res = make_thumbnail(&job);
            if res.is_none() {
                record_failure(&job);
            }
            (job.uri, job.mtime, job.size, res)
        },
        done_tx,
    );
    glib::MainContext::default().spawn_local(async move {
        while let Ok((uri, mtime, size, res)) = done_rx.recv().await {
            QUEUED.with(|q| q.borrow_mut().remove(&(uri.clone(), mtime, size)));
            if let Some(p) = res {
                forget_thumb(&p);
                thumbnail_ready(&uri, &p);
            }
        }
    });
    MAKERS.with(|m| *m.borrow_mut() = Some(q.clone()));
    q
}

/// Thumbnail files changed outside the makers (moved, copied or removed
/// with their files, see thumbcache): what's loaded of them is forgotten,
/// and the views told of the ones written (`uri`, path).
pub fn thumbnails_changed(written: &[(String, String)], removed: &[String]) {
    for p in removed.iter().chain(written.iter().map(|(_, p)| p)) {
        forget_thumb(p);
    }
    for (uri, p) in written {
        thumbnail_ready(uri, p);
    }
}

/// Tells the views that `uri`'s thumbnail is at `path`.
fn thumbnail_ready(uri: &str, path: &str) {
    LISTENERS.with(|l| {
        for (_, f) in l.borrow().iter() {
            f(uri, path);
        }
    });
}

/// Where Arkha notes the files it couldn't make a thumbnail of, as the
/// thumbnail spec has it, so they aren't tried every time they're shown.
fn fail_dir() -> std::path::PathBuf {
    glib::user_cache_dir().join("thumbnails").join("fail").join("arkha")
}

fn fail_path_for(uri: &str) -> std::path::PathBuf {
    let large = thumb_path_for(uri);
    fail_dir().join(large.file_name().unwrap_or_default())
}

/// A tiny picture with the file's URI, modification time and size: a
/// changed file is tried again. (The time alone wasn't enough: it's in
/// seconds, and a file still arriving that failed in the second its copy
/// ended had the finished file's time, so it was never tried again. Its
/// size was smaller.)
fn record_failure(job: &ThumbJob) {
    let out = fail_path_for(&job.uri);
    let Some(pb) = Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, true, 8, 1, 1) else { return };
    pb.fill(0);
    if std::fs::create_dir_all(fail_dir()).is_err() {
        return;
    }
    // The notes name files: only for their owner, as the thumbnails.
    let fail = fail_dir();
    for d in [Some(fail.as_path()), fail.parent(), fail.parent().and_then(|p| p.parent())].into_iter().flatten() {
        private(d, 0o700);
    }
    let (mtime, size) = (job.mtime.to_string(), job.size.to_string());
    if let Err(e) = pb.savev(&out, "png", &[("tEXt::Thumb::URI", job.uri.as_str()), ("tEXt::Thumb::MTime", mtime.as_str()), ("tEXt::Thumb::Size", size.as_str()), ("tEXt::Software", "Arkha")]) {
        glib::g_debug!("arkha", "thumbnail: couldn't note the failure in {}: {e}", out.display());
    }
    private(&out, 0o600);
}

fn make_thumbnail(job: &ThumbJob) -> Option<String> {
    // First: a thumbnailer writes its picture there too.
    if let Some(d) = job.out.parent() {
        if let Err(e) = std::fs::create_dir_all(d) {
            glib::g_debug!("arkha", "thumbnail: couldn't make {}: {e}", d.display());
            return None;
        }
        // As the thumbnail spec asks: the folders only for their owner
        // (thumbnails/ itself too), and the pictures (below).
        private(d, 0o700);
        if let Some(root) = d.parent() {
            private(root, 0o700);
        }
    }
    let pb = match &job.custom {
        // The command writes some image; it's then stored like any other
        // thumbnail (scaled, with the Thumb::URI/MTime the cache needs).
        Some(t) => {
            let raw = job.out.with_extension(format!("arkha-{}.raw.png", std::process::id()));
            glib::g_debug!("arkha", "thumbnail: running “{}” for {}", t.name, job.uri);
            let start = std::time::Instant::now();
            let ran = thumbnailers::run(t, &job.src, &raw, 256);
            let pb = ran
                .map_err(|e| format!("“{}” failed after {:.1} s: {e}", t.name, start.elapsed().as_secs_f64()))
                .and_then(|()| load_at_scale(&raw, 256).map_err(|e| format!("couldn't load what “{}” made: {e}", t.name)));
            let _ = std::fs::remove_file(&raw);
            match pb {
                Ok(pb) => pb,
                Err(e) => {
                    glib::g_debug!("arkha", "thumbnail: {e} ({})", job.uri);
                    return None;
                }
            }
        }
        None => {
            let _decoding = DECODE_BUDGET.take(decode_cost(&job.src));
            match load_at_scale(&job.src, 256) {
                Ok(pb) => pb.apply_embedded_orientation().unwrap_or(pb),
                Err(e) => {
                    glib::g_debug!("arkha", "thumbnail: couldn't load the image: {e} ({})", job.uri);
                    return None;
                }
            }
        }
    };
    let tmp = job.out.with_extension(format!("arkha-{}.tmp", std::process::id()));
    let mtime = job.mtime.to_string();
    let saved = pb
        .savev(
            &tmp,
            "png",
            &[
                ("tEXt::Thumb::URI", job.uri.as_str()),
                ("tEXt::Thumb::MTime", mtime.as_str()),
                ("tEXt::Software", "Arkha"),
                // About 2 ms instead of 12–26 for 256 px, the file about
                // as small (from a photo's 82 KB to 79, a drawing's 48 to 58).
                ("compression", "1"),
            ],
        )
        .map_err(|e| e.to_string())
        .and_then(|()| {
            private(&tmp, 0o600);
            std::fs::rename(&tmp, &job.out).map_err(|e| e.to_string())
        });
    if let Err(e) = saved {
        glib::g_debug!("arkha", "thumbnail: couldn't save {}: {e} ({})", job.out.display(), job.uri);
        return None;
    }
    glib::g_debug!("arkha", "thumbnail: made {} ({})", job.out.display(), job.uri);
    Some(job.out.to_string_lossy().to_string())
}

/// Sets `path`'s permissions to `mode` (0o700 for the thumbnail folders,
/// 0o600 for the files in them).
fn private(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

/// Memory for the pictures being decoded for thumbnails at once. Decoding
/// one takes some 20 bytes a pixel until it's scaled down: 3–4 in Arkha,
/// 8–27 in gdk-pixbuf's (glycin) loader process, by format. With all
/// makers busy on big pictures that added up to gigabytes (3 GB for
/// GNOME's 4096 × 4096 wallpapers). A maker waits while what it would
/// decode doesn't fit beside the others'; one bigger than all of it is
/// decoded on its own.
static DECODE_BUDGET: std::sync::LazyLock<DecodeBudget> =
    std::sync::LazyLock::new(|| DecodeBudget::new(decode_budget_for(total_ram().unwrap_or(8 << 30))));

/// A quarter of the memory, 1 to 4 GB: with 16 GB, the makers' 7 wallpapers
/// of 4096 × 4096 (335 MB each) or 4 photos of 50 megapixels at once; with
/// 8 GB, 6 such wallpapers or 2 photos; with 4 GB, 3 wallpapers or 1 photo.
fn decode_budget_for(ram: u64) -> usize {
    (ram / 4).clamp(1 << 30, 4 << 30) as usize
}

fn total_ram() -> Option<u64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb: u64 = info.lines().find_map(|l| l.strip_prefix("MemTotal:"))?.split_whitespace().next()?.parse().ok()?;
    Some(kb * 1024)
}

/// The memory decoding `path` takes (see DECODE_BUDGET), from its size in
/// its header; a picture whose size can't be told counts as a 16-megapixel
/// photo.
fn decode_cost(path: &std::path::Path) -> usize {
    crate::imagesize::dimensions(path).map_or(16_000_000, |(w, h)| w as usize * h as usize) * 20
}

struct DecodeBudget {
    limit: usize,
    used: Mutex<usize>,
    freed: Condvar,
}

/// A share of the budget, given back when dropped.
struct Decoding<'a> {
    budget: &'a DecodeBudget,
    cost: usize,
}

impl DecodeBudget {
    fn new(limit: usize) -> Self {
        DecodeBudget { limit, used: Mutex::new(0), freed: Condvar::new() }
    }

    fn take(&self, cost: usize) -> Decoding<'_> {
        let mut used = self.used.lock().unwrap_or_else(|e| e.into_inner());
        while *used > 0 && *used + cost > self.limit {
            used = self.freed.wait(used).unwrap_or_else(|e| e.into_inner());
        }
        *used += cost;
        Decoding { budget: self, cost }
    }
}

impl Drop for Decoding<'_> {
    fn drop(&mut self) {
        *self.budget.used.lock().unwrap_or_else(|e| e.into_inner()) -= self.cost;
        self.budget.freed.notify_all();
    }
}

/// A picture scaled to fit in `size` (as Pixbuf::from_file_at_scale). If
/// gdk-pixbuf can't tell its format from its content, by the file name's
/// extension: gdk-pixbuf 2.42 goes only by what the MIME database makes of
/// the content, and a wrong rule there makes it give up. One such rule is an
/// old DTS-HD one some programs left in users' MIME databases: any file with
/// "dX %" in its first 18 KB, some PNG thumbnails too.
pub fn load_at_scale(path: &std::path::Path, size: i32) -> Result<Pixbuf, glib::Error> {
    match Pixbuf::from_file_at_scale(path, size, size, true) {
        Err(e) if e.matches(gdk_pixbuf::PixbufError::UnknownType) => load_by_extension(path, size).ok_or(e),
        r => r,
    }
}

fn load_by_extension(path: &std::path::Path, size: i32) -> Option<Pixbuf> {
    let ext = path.extension()?.to_str()?;
    let format = Pixbuf::formats().into_iter().find(|f| f.extensions().iter().any(|e| e.eq_ignore_ascii_case(ext)))?;
    // Read first: a loader has to be closed before it's dropped (else
    // gdk-pixbuf warns), whatever happens.
    let data = std::fs::read(path).ok()?;
    let loader = gdk_pixbuf::PixbufLoader::with_type(format.name()?.as_str()).ok()?;
    loader.connect_size_prepared(move |l, w, h| {
        if w > 0 && h > 0 {
            let scale = (size as f64 / w as f64).min(size as f64 / h as f64);
            l.set_size(((w as f64 * scale).round() as i32).max(1), ((h as f64 * scale).round() as i32).max(1));
        }
    });
    let written = loader.write(&data);
    let closed = loader.close();
    written.and(closed).ok()?;
    loader.pixbuf()
}

fn builtin_can(ctype: &str) -> bool {
    ctype.starts_with("image/") && !ctype.contains("djvu") && !ctype.contains("x-xcf")
}

/// Content types we can make thumbnails for: images, plus whatever the
/// custom thumbnailers handle.
pub fn can_thumbnail(ctype: &str) -> bool {
    builtin_can(ctype) || custom_for(ctype).is_some()
}

/// Ask for a thumbnail to be generated (async). Results come via on_thumbnail_ready.
/// A custom thumbnailer for the type is used before the built-in one.
pub fn request_thumbnail(uri: &str, path: &std::path::Path, mtime: i64, size: u64, ctype: &str) {
    let custom = custom_for(ctype);
    // Big images are slow to decode; custom thumbnailers (videos…) decide themselves.
    if custom.is_none() && (!builtin_can(ctype) || size > crate::config::with(|s| s.thumbnail_limit)) {
        glib::g_debug!("arkha", "thumbnail: none for {ctype} of {size} bytes ({uri})");
        return;
    }
    let out = thumb_path_for(uri);
    // Don't thumbnail our own cache.
    if path.starts_with(glib::user_cache_dir().join("thumbnails")) {
        return;
    }
    // Made already (asked again before the view had it, or by another
    // view): the views get it as when it's made, once the view asking is
    // done with what it's doing.
    if png_thumb_mtime(&out) == Some(mtime.to_string()) {
        let (uri, p) = (uri.to_string(), out.to_string_lossy().to_string());
        glib::idle_add_local_once(move || thumbnail_ready(&uri, &p));
        return;
    }
    if failed_before(&fail_path_for(uri), mtime, size) {
        glib::g_debug!("arkha", "thumbnail: failed before, not tried again until the file changes ({uri})");
        return;
    }
    if !QUEUED.with(|q| q.borrow_mut().insert((uri.to_string(), mtime, size))) {
        return;
    }
    glib::g_debug!("arkha", "thumbnail: asked for {ctype} ({uri})");
    makers().push(ThumbJob {
        uri: uri.to_string(),
        src: path.to_path_buf(),
        mtime,
        size,
        out,
        custom,
    });
}

/// A usable existing thumbnail for this file, if the cache has a fresh one.
pub fn existing_thumbnail(info: &gio::FileInfo, uri: &str, mtime: i64) -> Option<String> {
    if let Some(p) = info.attribute_byte_string("thumbnail::path") {
        let valid = if info.has_attribute("thumbnail::is-valid") {
            info.boolean("thumbnail::is-valid")
        } else {
            true
        };
        if valid && !p.is_empty() {
            return Some(p.to_string());
        }
    }
    // Check our own location (older GLib doesn't report thumbnail::path for large).
    let p = thumb_path_for(uri);
    if png_thumb_mtime(&p) == Some(mtime.to_string()) {
        return Some(p.to_string_lossy().to_string());
    }
    None
}

/// Whether the failure noted at `path` is for the file as it is: the same
/// modification time and size. Notes without a size (made before Arkha
/// noted it) don't count: such a file is tried once more.
fn failed_before(path: &std::path::Path, mtime: i64, size: u64) -> bool {
    png_thumb_mtime(path) == Some(mtime.to_string()) && png_thumb_text(path, "Thumb::Size") == Some(size.to_string())
}

/// The Thumb::MTime text of a thumbnail PNG.
fn png_thumb_mtime(path: &std::path::Path) -> Option<String> {
    png_thumb_text(path, "Thumb::MTime")
}

/// A thumbnail PNG's text called `key`, read from the chunks before the
/// image data so we don't decode the picture on the main thread.
fn png_thumb_text(path: &std::path::Path, key: &str) -> Option<String> {
    use std::io::Read;
    let mut f = std::io::BufReader::new(std::fs::File::open(path).ok()?);
    let mut sig = [0u8; 8];
    f.read_exact(&mut sig).ok()?;
    if sig != *b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    loop {
        let mut head = [0u8; 8];
        f.read_exact(&mut head).ok()?;
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let kind = &head[4..8];
        if kind == b"IDAT" || kind == b"IEND" || len > 1 << 20 {
            return None;
        }
        let mut data = vec![0u8; len + 4]; // chunk data + CRC
        f.read_exact(&mut data).ok()?;
        if kind == b"tEXt" {
            let text = &data[..len];
            if let Some(i) = text.iter().position(|b| *b == 0) {
                if &text[..i] == key.as_bytes() {
                    return Some(String::from_utf8_lossy(&text[i + 1..]).to_string());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn sizes_costs_and_failure_notes() {
        // Fitted into a square, never down to nothing.
        assert_eq!(fit(400, 200, 100), (100, 50));
        assert_eq!(fit(200, 400, 100), (50, 100));
        assert_eq!(fit(3000, 2, 256), (256, 1));
        assert_eq!(fit(50, 50, 256), (256, 256));
        // Decoding costs by the size in a picture's header; unknown, as a
        // 16-megapixel photo.
        let d = TestDir::new("decodecost");
        let png = d.0.join("p.png");
        Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, false, 8, 30, 20).unwrap().savev(&png, "png", &[]).unwrap();
        assert_eq!(decode_cost(&png), 30 * 20 * 20);
        assert_eq!(decode_cost(&d.0.join("gone.png")), 16_000_000 * 20);
        assert_eq!(decode_cost(&d.file("not.png", "text")), 16_000_000 * 20);
        assert!(total_ram().is_some_and(|r| r > 64 << 20));
        // A failure is noted under the large thumbnail's name, in Arkha's
        // own folder of failures.
        let uri = "file:///home/u/a%20b.mp4";
        let note = fail_path_for(uri);
        assert_eq!(note.file_name(), thumb_path_for(uri).file_name());
        assert!(note.ends_with(std::path::Path::new("thumbnails/fail/arkha").join(note.file_name().unwrap())));
    }

    #[test]
    fn icons_with_custom_pictures_and_emblems() {
        let info = gio::FileInfo::new();
        info.set_icon(&gio::ThemedIcon::new("text-x-generic"));
        // Nothing given: the file's own icon, not marked custom.
        let plain = item_icon(&info, &[]).unwrap();
        assert!(!is_custom(&plain));
        assert_eq!(IconExt::to_string(&plain).unwrap().as_str(), IconExt::to_string(&gio::ThemedIcon::new("text-x-generic")).unwrap().as_str());
        // Emblems as Caja keeps them, a note's, and a link's first.
        info.set_attribute_stringv("metadata::emblems", ["important", "", "favorite"].as_slice());
        info.set_attribute_string("metadata::annotation", "remember");
        let with = item_icon(&info, &["emblem-symbolic-link"]).unwrap();
        let em = with.downcast_ref::<gio::EmblemedIcon>().expect("emblemed");
        let names: Vec<String> = em.emblems().iter().map(|e| IconExt::to_string(&e.icon()).unwrap().to_string()).collect();
        assert_eq!(names.len(), 4, "{names:?}");
        for n in ["emblem-symbolic-link", "emblem-note", "emblem-important", "emblem-favorite"] {
            assert!(names.iter().any(|x| x.contains(n)), "{n} in {names:?}");
        }
        assert!(!is_custom(&with));
        // A custom picture goes first, and is marked (no thumbnail over it).
        info.set_attribute_string("metadata::custom-icon", "file:///pictures/logo.png");
        let custom = item_icon(&info, &[]).unwrap();
        assert!(is_custom(&custom));
        let base = custom.downcast_ref::<gio::EmblemedIcon>().unwrap().icon();
        assert!(base.downcast_ref::<gio::FileIcon>().is_some_and(|f| f.file().uri() == "file:///pictures/logo.png"));
        // A theme icon's name works too.
        let named = gio::FileInfo::new();
        named.set_icon(&gio::ThemedIcon::new("folder"));
        named.set_attribute_string("metadata::custom-icon-name", "folder-music");
        let n = item_icon(&named, &[]).unwrap();
        assert!(is_custom(&n) && IconExt::to_string(&n).unwrap().contains("folder-music"));
    }

    #[test]
    fn text_starts_for_icons() {
        let d = TestDir::new("textstart");
        let lines: String = (1..=60).map(|i| format!("line {i}\tend\n")).collect();
        std::fs::write(d.0.join("a.txt"), &lines).unwrap();
        let t = read_text_start(&d.0.join("a.txt"));
        assert_eq!(t.lines().count(), 40);
        assert_eq!(t.lines().next(), Some("line 1    end"));
        std::fs::write(d.0.join("b.txt"), b"PK\x03\x04\x00binary").unwrap();
        assert_eq!(read_text_start(&d.0.join("b.txt")), "");
        assert_eq!(read_text_start(&d.0.join("missing")), "");
        assert!(is_text("text/plain") && is_text("text/x-shellscript") && !is_text("image/png"));
    }

    #[test]
    fn emblem_titles() {
        assert_eq!(emblem_title("emblem-important"), "Important");
        assert_eq!(emblem_title("emblem-new_file-x"), "New file x");
        assert_eq!(emblem_title("odd"), "Odd");
    }

    #[test]
    fn pictures_load_by_extension_too() {
        let d = TestDir::new("loadext");
        let pb = Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, false, 8, 40, 20).unwrap();
        pb.fill(0x336699ff);
        let png = d.0.join("t.raw.png");
        pb.savev(&png, "png", &[]).unwrap();
        let fit = |p: Pixbuf| (p.width(), p.height());
        assert_eq!(load_at_scale(&png, 10).map(fit).unwrap(), (10, 5));
        // What's used when the content isn't recognized: scaled alike.
        assert_eq!(load_by_extension(&png, 10).map(fit), Some((10, 5)));
        assert_eq!(load_by_extension(&png, 80).map(fit), Some((80, 40)));
        // No format for the name: nothing to fall back on.
        let other = d.0.join("t.mystery");
        std::fs::copy(&png, &other).unwrap();
        assert_eq!(load_by_extension(&other, 10).map(fit), None);
        let text = d.0.join("t2.png");
        std::fs::write(&text, "not a picture").unwrap();
        assert!(load_at_scale(&text, 10).is_err());
        // Not a picture, or gone: nothing, and the loader closed before
        // it's dropped (gdk-pixbuf warns about one that isn't).
        let unclosed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = unclosed.clone();
        let handler = glib::log_set_handler(Some("GdkPixbuf"), glib::LogLevels::LEVEL_WARNING, false, false, move |_, _, message| {
            if message.contains("without calling gdk_pixbuf_loader_close") {
                seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        });
        assert_eq!(load_by_extension(&text, 10).map(fit), None);
        assert_eq!(load_by_extension(&d.0.join("gone.png"), 10).map(fit), None);
        glib::log_remove_handler(Some("GdkPixbuf"), handler);
        assert_eq!(unclosed.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn thumbnail_caches_stay_within_their_memory() {
        THUMB_BYTES.with(|c| c.set(0));
        // Fits: counted.
        assert!(!make_room(&THUMB_BYTES, 0, CACHED_BYTES - 10));
        assert!(!make_room(&THUMB_BYTES, 1, 10));
        // One byte more: emptied, and the count starts with this picture.
        assert!(make_room(&THUMB_BYTES, 2, 1));
        assert_eq!(THUMB_BYTES.with(|c| c.get()), 1);
        // Too many entries, however small.
        assert!(make_room(&THUMB_BYTES, 4000, 1));
        assert_eq!(THUMB_BYTES.with(|c| c.get()), 1);
    }

    /// A PNG's signature and chunks. The CRCs are left zero: they aren't checked.
    fn png(chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        for (kind, data) in chunks {
            v.extend((data.len() as u32).to_be_bytes());
            v.extend(*kind);
            v.extend(*data);
            v.extend([0u8; 4]);
        }
        v
    }

    #[test]
    fn header_tells_size_and_transparency() {
        // 300×200, 8 bits, the given colour type.
        let ihdr = |colour: u8| -> Vec<u8> { [&300u32.to_be_bytes()[..], &200u32.to_be_bytes()[..], &[8, colour, 0, 0, 0]].concat() };
        let head = |chunks: &[(&[u8; 4], &[u8])]| parse_png_header(&png(chunks));
        let (rgba, rgb) = (ihdr(6), ihdr(2));
        assert_eq!(head(&[(b"IHDR", &rgba), (b"IDAT", b"")]), Some(ThumbHeader { width: 300, height: 200, alpha: true }));
        assert_eq!(head(&[(b"IHDR", &rgb), (b"tEXt", b"Thumb::MTime\x001"), (b"IDAT", b"")]).map(|h| h.alpha), Some(false));
        assert_eq!(head(&[(b"IHDR", &rgb), (b"tRNS", b"\0\0"), (b"IDAT", b"")]).map(|h| h.alpha), Some(true));
        // A tRNS after the picture's data isn't one.
        assert_eq!(head(&[(b"IHDR", &rgb), (b"IDAT", b""), (b"tRNS", b"\0\0")]).map(|h| h.alpha), Some(false));
        assert_eq!(parse_png_header(b"GIF89a"), None);
    }

    #[test]
    fn decode_budget_sizes() {
        assert_eq!(decode_budget_for(16 << 30), 4 << 30);
        assert_eq!(decode_budget_for(8 << 30), 2 << 30);
        assert_eq!(decode_budget_for(2 << 30), 1 << 30);
        assert_eq!(decode_budget_for(256 << 30), 4 << 30);
    }

    #[test]
    fn decode_budget_holds_back_what_doesnt_fit() {
        let b = Arc::new(DecodeBudget::new(100));
        let first = b.take(60);
        let small = b.take(40); // fits beside it
        let got = Arc::new(AtomicBool::new(false));
        let waiter = {
            let (b, got) = (b.clone(), got.clone());
            std::thread::spawn(move || {
                let _big = b.take(500); // bigger than it all: alone
                got.store(true, Ordering::SeqCst);
            })
        };
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(!got.load(Ordering::SeqCst), "waits while others decode");
        drop(first);
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!got.load(Ordering::SeqCst), "still one decoding");
        drop(small);
        waiter.join().unwrap();
        assert!(got.load(Ordering::SeqCst));
        assert_eq!(*b.used.lock().unwrap(), 0);
    }

    #[test]
    fn thumbnail_path() {
        // The example in the freedesktop.org thumbnail spec.
        let p = thumb_path_for("file:///home/jens/photos/me.png");
        assert!(p.ends_with("thumbnails/large/c6ee772d9e49320e97ec29a7eb5b1697.png"));
    }

    #[test]
    fn thumbnail_mtime() {
        let t = TestDir::new("png");
        let write = |name: &str, bytes: &[u8]| {
            let p = t.0.join(name);
            std::fs::write(&p, bytes).unwrap();
            p
        };
        let ihdr: &[u8] = &[0; 13];
        let good = png(&[(b"IHDR", ihdr), (b"tEXt", b"Software\0GIMP"), (b"tEXt", b"Thumb::MTime\x001700000000"), (b"IDAT", b"")]);
        assert_eq!(png_thumb_mtime(&write("good.png", &good)).as_deref(), Some("1700000000"));
        // Only the chunks before the picture data are read.
        let late = png(&[(b"IHDR", ihdr), (b"IDAT", b""), (b"tEXt", b"Thumb::MTime\x001")]);
        assert_eq!(png_thumb_mtime(&write("late.png", &late)), None);
        let without = png(&[(b"IHDR", ihdr), (b"tEXt", b"Thumb::URI\0file:///x"), (b"IEND", b"")]);
        assert_eq!(png_thumb_mtime(&write("without.png", &without)), None);
        assert_eq!(png_thumb_mtime(&write("cut.png", &good[..good.len() / 2])), None);
        assert_eq!(png_thumb_mtime(&write("jpeg.png", b"\xff\xd8\xff\xe0 not a png")), None);
        assert_eq!(png_thumb_mtime(&t.0.join("missing.png")), None);
    }

    #[test]
    fn failures_noted_with_time_and_size() {
        let t = TestDir::new("fail");
        let ihdr: &[u8] = &[0; 13];
        let note = t.0.join("note.png");
        std::fs::write(&note, png(&[(b"IHDR", ihdr), (b"tEXt", b"Thumb::MTime\x001700000000"), (b"tEXt", b"Thumb::Size\x004096"), (b"IDAT", b"")])).unwrap();
        assert!(failed_before(&note, 1_700_000_000, 4096));
        // Finished in the same second, but bigger than when it failed.
        assert!(!failed_before(&note, 1_700_000_000, 24_324_302));
        assert!(!failed_before(&note, 1_700_000_001, 4096));
        // A note from before sizes were noted: tried once more.
        let old = t.0.join("old.png");
        std::fs::write(&old, png(&[(b"IHDR", ihdr), (b"tEXt", b"Thumb::MTime\x001700000000"), (b"IDAT", b"")])).unwrap();
        assert!(!failed_before(&old, 1_700_000_000, 4096));
        assert!(!failed_before(&t.0.join("none.png"), 1_700_000_000, 4096));
    }

    #[test]
    fn existing_thumbnails() {
        let i = gio::FileInfo::new();
        let uri = "file:///no/such/arkha-test-file.png";
        assert_eq!(existing_thumbnail(&i, uri, 5), None);
        i.set_attribute_byte_string("thumbnail::path", "/x/t.png");
        assert_eq!(existing_thumbnail(&i, uri, 5).as_deref(), Some("/x/t.png"));
        // An outdated one isn't used.
        i.set_attribute_boolean("thumbnail::is-valid", false);
        assert_eq!(existing_thumbnail(&i, uri, 5), None);
    }

    #[test]
    fn makers_leave_a_processor_free() {
        let counts: Vec<usize> = [1, 2, 4, 8, 9, 16, 64].iter().map(|&n| makers_for(n)).collect();
        assert_eq!(counts, [1, 1, 3, 7, 8, 8, 8]);
    }
}
