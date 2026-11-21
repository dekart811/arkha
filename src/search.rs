//! Searching, as in Caja: what's typed in the search bar (Ctrl+F) is
//! looked for in the names of everything below a folder, and the results
//! are a place of their own in a pane's history, with Caja's query editor
//! above them (more criteria: location, file type, tags, modification
//! time, size, contained text).

use crate::i18n::{fill, n_, tr};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::glib::translate::from_glib_full;
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::pane::ModelGet;

// ------------------------------------------------------------------ query

/// The kinds of criteria, in the order of Caja's row type menu (these
/// tables' texts are translated where shown, in text_combo).
pub const KINDS: [&str; 6] = [n_("Location"), n_("File Type"), n_("Tags"), n_("Modification Time"), n_("Size"), n_("Contained text")];

/// Caja's file type groups and their MIME types.
const GROUPS: &[(&str, &[&str])] = &[
    (n_("Documents"), &["application/rtf", "application/msword", "application/vnd.sun.xml.writer", "application/vnd.sun.xml.writer.global", "application/vnd.sun.xml.writer.template", "application/vnd.oasis.opendocument.text", "application/vnd.oasis.opendocument.text-template", "application/x-abiword", "application/x-applix-word", "application/x-mswrite", "application/docbook+xml", "application/x-kword", "application/x-kword-crypt", "application/x-lyx"]),
    (n_("Music"), &["application/ogg", "audio/ac3", "audio/basic", "audio/midi", "audio/x-flac", "audio/mp4", "audio/mpeg", "audio/x-mpeg", "audio/x-ms-asx", "audio/x-pn-realaudio"]),
    (n_("Video"), &["video/mp4", "video/3gpp", "video/mpeg", "video/quicktime", "video/vivo", "video/x-avi", "video/x-mng", "video/x-ms-asf", "video/x-ms-wmv", "video/x-msvideo", "video/x-nsv", "video/x-real-video"]),
    (n_("Picture"), &["application/vnd.oasis.opendocument.image", "application/x-krita", "image/bmp", "image/cgm", "image/gif", "image/jpeg", "image/jpeg2000", "image/png", "image/svg+xml", "image/tiff", "image/x-compressed-xcf", "image/x-pcx", "image/x-photo-cd", "image/x-psd", "image/x-tga", "image/x-xcf"]),
    (n_("Illustration"), &["application/illustrator", "application/vnd.corel-draw", "application/vnd.stardivision.draw", "application/vnd.oasis.opendocument.graphics", "application/x-dia-diagram", "application/x-karbon", "application/x-killustrator", "application/x-kivio", "application/x-kontour", "application/x-wpg"]),
    (n_("Spreadsheet"), &["application/vnd.lotus-1-2-3", "application/vnd.ms-excel", "application/vnd.stardivision.calc", "application/vnd.sun.xml.calc", "application/vnd.oasis.opendocument.spreadsheet", "application/x-applix-spreadsheet", "application/x-gnumeric", "application/x-kspread", "application/x-kspread-crypt", "application/x-quattropro", "application/x-sc", "application/x-siag"]),
    (n_("Presentation"), &["application/vnd.ms-powerpoint", "application/vnd.sun.xml.impress", "application/vnd.oasis.opendocument.presentation", "application/x-magicpoint", "application/x-kpresenter"]),
    (n_("Pdf / Postscript"), &["application/pdf", "application/postscript", "application/x-dvi", "image/x-eps"]),
    (n_("Text File"), &["text/plain"]),
];

const COMPARISONS: [&str; 2] = [n_("Less than or equal to"), n_("Greater than or equal to")];
const DURATIONS: [&str; 6] = [n_("1 Hour"), n_("1 Day"), n_("1 Week"), n_("1 Month"), n_("6 Months"), n_("1 Year")];
const SIZES_SI: [(i64, &str); 11] = [(10_000, "10 KB"), (100_000, "100 KB"), (500_000, "500 KB"), (1_000_000, "1 MB"), (5_000_000, "5 MB"), (10_000_000, "10 MB"), (100_000_000, "100 MB"), (500_000_000, "500 MB"), (1_000_000_000, "1 GB"), (2_000_000_000, "2 GB"), (4_000_000_000, "4 GB")];
const SIZES_IEC: [(i64, &str); 11] = [(10_240, "10 KiB"), (102_400, "100 KiB"), (512_000, "500 KiB"), (1_048_576, "1 MiB"), (5_242_880, "5 MiB"), (10_485_760, "10 MiB"), (104_857_600, "100 MiB"), (524_288_000, "500 MiB"), (1_073_741_824, "1 GiB"), (2_147_483_648, "2 GiB"), (4_294_967_296, "4 GiB")];

fn sizes() -> &'static [(i64, &'static str); 11] {
    if crate::config::with(|s| s.iec_units) {
        &SIZES_IEC
    } else {
        &SIZES_SI
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FileType {
    Any,
    /// One of Caja's groups (an index into GROUPS).
    Group(usize),
    /// A type picked with "Other Type...": its MIME type and description.
    Other(String, String),
}

/// One of the query editor's rows.
#[derive(Clone, Debug, PartialEq)]
pub enum Criterion {
    /// The folder to search (a URI), and whether the folders in it too.
    Location { uri: String, subfolders: bool },
    FileType(FileType),
    /// Tags separated by spaces; files need all of them.
    Tags(String),
    /// At most / at least (`greater`) that old: an index into DURATIONS.
    ModTime { greater: bool, duration: usize },
    /// At most / at least (`greater`) that big: an index into the sizes.
    Size { greater: bool, size: usize },
    ContainedText(String),
}

impl Criterion {
    /// A row of kind `kind` (an index into KINDS) as Caja adds it; the
    /// location is `here`.
    fn new(kind: usize, here: &str) -> Criterion {
        match kind {
            0 => Criterion::location(here),
            1 => Criterion::FileType(FileType::Any),
            2 => Criterion::Tags(String::new()),
            3 => Criterion::ModTime { greater: false, duration: 0 },
            4 => Criterion::Size { greater: false, size: 0 },
            _ => Criterion::ContainedText(String::new()),
        }
    }

    /// A location row for `here`, with subfolders as Preferences say.
    fn location(here: &str) -> Criterion {
        Criterion::Location { uri: here.to_string(), subfolders: crate::config::with(|s| s.search_subfolders) }
    }

    fn kind(&self) -> usize {
        match self {
            Criterion::Location { .. } => 0,
            Criterion::FileType(_) => 1,
            Criterion::Tags(_) => 2,
            Criterion::ModTime { .. } => 3,
            Criterion::Size { .. } => 4,
            Criterion::ContainedText(_) => 5,
        }
    }
}

/// A search: the text looked for in names, and the query editor's rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub text: String,
    pub criteria: Vec<Criterion>,
}

impl Query {
    /// A search for `text` from the search bar, in the folder `here`.
    pub fn new(text: &str, here: &gio::File) -> Query {
        Query { text: text.to_string(), criteria: vec![Criterion::location(&here.uri())] }
    }

    /// Caja's title for the results.
    pub fn title(&self) -> String {
        fill(&tr("Search for “{text}”"), &[("text", &self.text)])
    }

    /// What the search engine needs, with the times counted from now. As
    /// in Caja, a later row of a kind overrides an earlier one, except
    /// file types and tags, which add up; with no location row it's the
    /// whole file system.
    pub fn params(&self) -> Params {
        let mut p = Params { root: None, subfolders: true, words: fold(&self.text).split(' ').map(str::to_string).collect(), mimes: vec![], tags: vec![], timestamp: 0, size: 0, text: None };
        for c in &self.criteria {
            match c {
                Criterion::Location { uri, subfolders } => (p.root, p.subfolders) = (Some(uri.clone()), *subfolders),
                Criterion::FileType(FileType::Any) => {}
                Criterion::FileType(FileType::Group(g)) => p.mimes.extend(GROUPS[*g].1.iter().map(|m| m.to_string())),
                Criterion::FileType(FileType::Other(mime, _)) => p.mimes.push(mime.clone()),
                Criterion::Tags(t) => p.tags.extend(t.split(' ').map(str::trim).filter(|t| !t.is_empty()).map(fold)),
                Criterion::ModTime { greater, duration } => {
                    let now = glib::DateTime::now_local().ok();
                    let then = now.and_then(|n| match duration {
                        0 => n.add_hours(-1).ok(),
                        1 => n.add_days(-1).ok(),
                        2 => n.add_weeks(-1).ok(),
                        3 => n.add_months(-1).ok(),
                        4 => n.add_months(-6).ok(),
                        _ => n.add_years(-1).ok(),
                    });
                    let t = then.map_or(0, |t| t.to_unix());
                    p.timestamp = if *greater { t } else { -t };
                }
                Criterion::Size { greater, size } => {
                    let s = sizes()[(*size).min(10)].0;
                    p.size = if *greater { s } else { -s };
                }
                Criterion::ContainedText(t) => p.text = Some(fold(t)),
            }
        }
        p
    }
}

// ----------------------------------------------------------------- engine

/// A search ready to run (see Query::params).
pub struct Params {
    /// The folder to search, a URI (None: the file system).
    root: Option<String>,
    /// The folders in it too (and theirs), or only what it holds itself.
    subfolders: bool,
    /// The words of the text, all of which a name must contain.
    words: Vec<String>,
    /// MIME types of which a file must be one (none: any).
    mimes: Vec<String>,
    /// Tags a file must all have.
    tags: Vec<String>,
    /// Modified at or before it (> 0), or at or after its negation (< 0).
    timestamp: i64,
    /// At least this big (> 0), or at most its negation (< 0).
    size: i64,
    /// Text a file must contain (plain text and OpenDocument files), folded.
    text: Option<String>,
}

impl Params {
    pub fn root(&self) -> gio::File {
        self.root.as_deref().map_or_else(|| gio::File::for_path("/"), gio::File::for_uri)
    }
}

/// `s` decomposed (NFD) and in lower case, as Caja compares names.
pub fn fold(s: &str) -> String {
    unsafe {
        let n = glib::ffi::g_utf8_normalize(s.as_ptr() as *const _, s.len() as isize, glib::ffi::G_NORMALIZE_NFD);
        if n.is_null() {
            return s.to_lowercase();
        }
        let d = glib::ffi::g_utf8_strdown(n, -1);
        glib::ffi::g_free(n as glib::ffi::gpointer);
        from_glib_full::<_, glib::GString>(d).to_string()
    }
}

/// Whether a name contains all the words (folded, see `fold`).
pub fn name_matches(words: &[String], name: &str) -> bool {
    let name = fold(name);
    words.iter().all(|w| name.contains(w.as_str()))
}

/// The tags in an xdg.tags attribute as GIO gives it (bytes outside ASCII
/// escaped as \xNN), folded.
pub fn tags_of(escaped: &str) -> Vec<String> {
    let b = escaped.as_bytes();
    let mut raw = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'\\' && b.get(i + 1) == Some(&b'x') && i + 3 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 2]), hex(b[i + 3])) {
                raw.push((h * 16 + l) as u8);
                i += 4;
                continue;
            }
        }
        raw.push(b[i]);
        i += 1;
    }
    fold(&String::from_utf8_lossy(&raw)).split(',').map(str::to_string).collect()
}

fn odt2txt_available() -> bool {
    std::process::Command::new("odt2txt").arg("--version").output().is_ok_and(|o| o.status.success() && o.stdout.starts_with(b"odt2txt"))
}

/// Whether the file at `path` contains `text` (folded), ignoring case
/// (plain text files, and OpenDocument files with odt2txt).
fn contains_text(path: &std::path::Path, text: &str, ctype: &str, odt2txt: bool) -> bool {
    if text.is_empty() {
        return true;
    }
    let contents = if gio::content_type_is_mime_type(ctype, "text/plain") {
        return file_contains(path, text);
    } else {
        if !odt2txt {
            return false;
        }
        match std::process::Command::new("odt2txt").arg(path).output() {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
            _ => return false,
        }
    };
    fold(&contents).contains(text)
}

/// Whether the text file at `path` contains `text` (folded), read a piece
/// at a time: a big log file mustn't take as much memory.
fn file_contains(path: &std::path::Path, text: &str) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    let mut buf = vec![0u8; 1 << 20];
    // Bytes of a character cut off at the end of a piece, and the end of
    // the text before (a match may run across pieces).
    let (mut cut, mut tail) = (Vec::new(), String::new());
    let keep = text.chars().count() * 2;
    loop {
        let n = match f.read(&mut buf) {
            Ok(0) | Err(_) => return false,
            Ok(n) => n,
        };
        cut.extend_from_slice(&buf[..n]);
        let valid = match std::str::from_utf8(&cut) {
            Ok(_) => cut.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(_) => cut.len(),
        };
        let piece = format!("{tail}{}", String::from_utf8_lossy(&cut[..valid]));
        if fold(&piece).contains(text) {
            return true;
        }
        tail = piece.chars().rev().take(keep).collect::<Vec<_>>().into_iter().rev().collect();
        cut.drain(..valid);
    }
}

const TEXT_TYPES: [&str; 6] = [
    "application/vnd.oasis.opendocument.text",
    "application/vnd.oasis.opendocument.text-template",
    "application/vnd.oasis.opendocument.spreadsheet",
    "application/vnd.oasis.opendocument.spreadsheet-template",
    "application/vnd.oasis.opendocument.presentation",
    "application/vnd.oasis.opendocument.presentation-template",
];

/// Runs a search (in a thread of its own), as Caja's simple search engine
/// does: breadth first from the root (or only in it, without subfolders),
/// hidden items left out (and not looked into), each folder visited once. The matches go to `found` as
/// (folder URI, name) pairs, a few hundred items' worth at a time; it
/// stops when `found` returns false or `go_on` does.
pub fn run(p: &Params, go_on: impl Fn() -> bool, mut found: impl FnMut(Vec<(String, String)>) -> bool) {
    let mut attrs = String::from("standard::name,standard::display-name,standard::is-hidden,standard::type,id::file");
    if !p.mimes.is_empty() || p.text.is_some() {
        attrs.push_str(",standard::content-type");
    }
    if !p.tags.is_empty() {
        attrs.push_str(",xattr::xdg.tags");
    }
    if p.timestamp != 0 {
        attrs.push_str(",time::modified");
    }
    if p.size != 0 {
        attrs.push_str(",standard::size");
    }
    let odt2txt = p.text.is_some() && odt2txt_available();
    let root = p.root();
    let mut visited = std::collections::HashSet::new();
    if let Some(id) = root.query_info("id::file", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE).ok().and_then(|i| i.attribute_string("id::file")) {
        visited.insert(id.to_string());
    }
    let mut todo = std::collections::VecDeque::from([root]);
    let mut hits = vec![];
    let mut processed = 0;
    while let Some(dir) = todo.pop_front() {
        if !go_on() {
            return;
        }
        let Ok(en) = dir.enumerate_children(&attrs, gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) else { continue };
        let dir_uri = dir.uri().to_string();
        while let Ok(Some(info)) = en.next_file(gio::Cancellable::NONE) {
            if info.is_hidden() {
                continue;
            }
            let mut hit = name_matches(&p.words, &info.display_name());
            // Only asked for when needed (GIO complains otherwise).
            let ctype = if info.has_attribute("standard::content-type") { info.content_type().map(|c| c.to_string()).unwrap_or_default() } else { String::new() };
            if hit && !p.mimes.is_empty() {
                hit = p.mimes.iter().any(|m| gio::content_type_equals(&ctype, m));
            }
            if hit && !p.tags.is_empty() {
                let have = info.attribute_string("xattr::xdg.tags").map(|t| tags_of(&t)).unwrap_or_default();
                hit = p.tags.iter().all(|t| have.contains(t));
            }
            if hit && p.timestamp != 0 {
                let mtime = info.attribute_uint64("time::modified") as i64;
                hit = if p.timestamp > 0 { mtime <= p.timestamp } else { mtime >= -p.timestamp };
            }
            if hit && p.size != 0 {
                let size = info.size();
                hit = if p.size > 0 { size >= p.size } else { size <= -p.size };
            }
            let name = info.name();
            if hit {
                if let Some(text) = &p.text {
                    hit = (gio::content_type_is_mime_type(&ctype, "text/plain") || TEXT_TYPES.iter().any(|t| gio::content_type_equals(&ctype, t)))
                        && dir.child(&name).path().is_some_and(|path| contains_text(&path, text, &ctype, odt2txt));
                }
            }
            if hit {
                hits.push((dir_uri.clone(), name.to_string_lossy().into_owned()));
            }
            processed += 1;
            if processed > 500 {
                processed = 0;
                if !hits.is_empty() && !found(std::mem::take(&mut hits)) {
                    return;
                }
            }
            if p.subfolders && info.file_type() == gio::FileType::Directory {
                let seen = info.attribute_string("id::file").is_some_and(|id| !visited.insert(id.to_string()));
                if !seen {
                    todo.push_back(dir.child(&name));
                }
            }
        }
    }
    if !hits.is_empty() {
        found(hits);
    }
}

// ----------------------------------------------------------------- editor

/// What's called when the rows change (with the criteria they're set to).
type ChangedFn = Box<dyn Fn(Vec<Criterion>)>;

/// A row of the query editor: the kind of criterion and its controls.
struct Row {
    hbox: gtk::Box,
    kind: Cell<usize>,
    widget: RefCell<gtk::Widget>,
}

/// Caja's query editor, shown above the search results: "Search results"
/// with Reload and +, then a row per criterion (its kind, its controls,
/// and − to remove it). Changes to the rows are kept (`on_changed`), the
/// search runs again with Reload or Enter (`on_reload`).
pub struct QueryEditor {
    pub widget: gtk::Frame,
    rows_box: gtk::Box,
    rows: RefCell<Vec<Rc<Row>>>,
    /// Where a new location row starts.
    here: RefCell<String>,
    frozen: Cell<bool>,
    on_changed: RefCell<Option<ChangedFn>>,
    on_reload: RefCell<Option<Box<dyn Fn()>>>,
}

fn icon_button(icons: &[&str], tip: &str) -> gtk::Button {
    let b = gtk::Button::new();
    b.add(&gtk::Image::from_gicon(&gio::ThemedIcon::from_names(icons), gtk::IconSize::SmallToolbar));
    b.set_relief(gtk::ReliefStyle::None);
    b.set_tooltip_text(Some(tip));
    b
}

fn text_combo(items: &[&str], active: usize) -> gtk::ComboBoxText {
    let c = gtk::ComboBoxText::new();
    for i in items {
        c.append_text(&tr(i));
    }
    c.set_active(Some(active as u32));
    c
}

impl QueryEditor {
    pub fn new() -> Rc<QueryEditor> {
        // As in a Caja window slot: an etched frame, then 6 pixels around
        // and between the editor's lines.
        let frame = gtk::Frame::new(None);
        frame.set_shadow_type(gtk::ShadowType::EtchedIn);
        let eventbox = gtk::EventBox::new();
        eventbox.set_widget_name("caja-extra-view-widget");
        frame.add(&eventbox);
        let extras = gtk::Box::new(gtk::Orientation::Vertical, 6);
        extras.set_border_width(6);
        eventbox.add(&extras);
        let rows_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        extras.pack_start(&rows_box, true, true, 0);
        let first = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        first.pack_start(&gtk::Label::new(Some(&tr("Search results"))), false, false, 0);
        let add = icon_button(&["add", "list-add"], &tr("Add a new criterion to this search"));
        first.pack_end(&add, false, false, 0);
        let reload = gtk::Button::with_label(&tr("Reload"));
        reload.set_tooltip_text(Some(&tr("Perform or update the search")));
        first.pack_end(&reload, false, false, 0);
        rows_box.pack_start(&first, false, false, 0);
        frame.show_all();
        frame.set_no_show_all(true);
        frame.hide();
        let ed = Rc::new(QueryEditor {
            widget: frame,
            rows_box,
            rows: RefCell::default(),
            here: RefCell::default(),
            frozen: Cell::new(false),
            on_changed: RefCell::default(),
            on_reload: RefCell::default(),
        });
        let w = Rc::downgrade(&ed);
        add.connect_clicked(move |_| {
            if let Some(ed) = w.upgrade() {
                ed.add_row(&Criterion::new(ed.next_free_kind(), &ed.here.borrow()));
                ed.changed();
            }
        });
        let w = Rc::downgrade(&ed);
        reload.connect_clicked(move |_| {
            if let Some(ed) = w.upgrade() {
                ed.reload();
            }
        });
        ed
    }

    pub fn connect_changed(&self, f: impl Fn(Vec<Criterion>) + 'static) {
        *self.on_changed.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_reload(&self, f: impl Fn() + 'static) {
        *self.on_reload.borrow_mut() = Some(Box::new(f));
    }

    fn changed(&self) {
        if self.frozen.get() {
            return;
        }
        if let Some(f) = self.on_changed.borrow().as_ref() {
            f(self.criteria());
        }
    }

    fn reload(&self) {
        if let Some(f) = self.on_reload.borrow().as_ref() {
            f();
        }
    }

    /// Shows `query`'s rows; a new location row starts at `here`.
    pub fn set_query(self: &Rc<Self>, query: &Query, here: &gio::File) {
        *self.here.borrow_mut() = here.uri().to_string();
        if self.criteria() == query.criteria {
            return;
        }
        self.frozen.set(true);
        for r in self.rows.borrow_mut().drain(..) {
            self.rows_box.remove(&r.hbox);
        }
        for c in &query.criteria {
            self.add_row(c);
        }
        self.frozen.set(false);
    }

    /// The first kind no row has yet (else File Type), as Caja's + adds.
    fn next_free_kind(&self) -> usize {
        let rows = self.rows.borrow();
        (0..KINDS.len()).find(|k| !rows.iter().any(|r| r.kind.get() == *k)).unwrap_or(1)
    }

    fn add_row(self: &Rc<Self>, c: &Criterion) {
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let combo = text_combo(&KINDS, c.kind());
        hbox.pack_start(&combo, false, false, 0);
        let remove = icon_button(&["remove", "list-remove"], &tr("Remove this criterion from the search"));
        hbox.pack_end(&remove, false, false, 0);
        let row = Rc::new(Row { hbox: hbox.clone(), kind: Cell::new(c.kind()), widget: RefCell::new(gtk::Label::new(None).upcast()) });
        self.set_row_widget(&row, c);
        hbox.show_all();
        self.rows_box.pack_start(&hbox, false, false, 0);
        self.rows.borrow_mut().push(row.clone());
        let (w, r) = (Rc::downgrade(self), Rc::downgrade(&row));
        combo.connect_changed(move |combo| {
            let (Some(ed), Some(row), Some(kind)) = (w.upgrade(), r.upgrade(), combo.active()) else { return };
            if kind as usize == row.kind.get() {
                return;
            }
            row.kind.set(kind as usize);
            ed.set_row_widget(&row, &Criterion::new(kind as usize, &ed.here.borrow()));
            ed.changed();
        });
        let (w, r) = (Rc::downgrade(self), Rc::downgrade(&row));
        remove.connect_clicked(move |_| {
            let (Some(ed), Some(row)) = (w.upgrade(), r.upgrade()) else { return };
            ed.rows_box.remove(&row.hbox);
            ed.rows.borrow_mut().retain(|x| !Rc::ptr_eq(x, &row));
            ed.changed();
        });
    }

    /// Puts the controls for `c` in `row`, in place of its old ones.
    fn set_row_widget(self: &Rc<Self>, row: &Rc<Row>, c: &Criterion) {
        let old = row.widget.borrow().clone();
        row.hbox.remove(&old);
        let w = Rc::downgrade(self);
        let changed = move || {
            if let Some(ed) = w.upgrade() {
                ed.changed();
            }
        };
        let w = Rc::downgrade(self);
        let reload = move || {
            if let Some(ed) = w.upgrade() {
                ed.reload();
            }
        };
        let (widget, expand): (gtk::Widget, bool) = match c {
            Criterion::Location { uri, subfolders } => {
                let bx = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                let chooser = gtk::FileChooserButton::new(&tr("Select folder to search in"), gtk::FileChooserAction::SelectFolder);
                chooser.set_local_only(false);
                let _ = chooser.set_current_folder_uri(uri);
                let changed = Rc::new(changed);
                let c = changed.clone();
                chooser.connect_current_folder_changed(move |_| c());
                bx.pack_start(&chooser, false, false, 0);
                // Unticked, only what the folder itself holds.
                let sub = gtk::CheckButton::with_label(&tr("including subfolders"));
                sub.set_active(*subfolders);
                sub.set_tooltip_text(Some(&tr("Also search the folders in this one, and theirs. Reload searches again.")));
                sub.connect_toggled(move |_| changed());
                bx.pack_start(&sub, false, false, 0);
                (bx.upcast(), false)
            }
            Criterion::FileType(t) => (self.type_combo(t, changed).upcast(), false),
            Criterion::Tags(text) | Criterion::ContainedText(text) => {
                let entry = gtk::Entry::new();
                let tip = if matches!(c, Criterion::Tags(_)) {
                    entry.set_max_length(4096);
                    tr("Tags separated by spaces. Matches files that contain ALL the tags.")
                } else {
                    tr("Matches files that contain the text.")
                };
                entry.set_tooltip_text(Some(&tip));
                entry.set_placeholder_text(Some(&tip));
                entry.set_text(text);
                let tags = matches!(c, Criterion::Tags(_));
                entry.connect_changed(move |e| {
                    // Tags are separated by spaces: commas go.
                    if tags && e.text().contains(',') {
                        e.set_text(&e.text().replace(',', ""));
                        return;
                    }
                    changed();
                });
                entry.connect_activate(move |_| reload());
                (entry.upcast(), true)
            }
            Criterion::ModTime { greater, duration: which } | Criterion::Size { greater, size: which } => {
                let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 7);
                let labels: Vec<&str> = if matches!(c, Criterion::ModTime { .. }) { DURATIONS.to_vec() } else { sizes().iter().map(|s| s.1).collect() };
                let cmp = text_combo(&COMPARISONS, *greater as usize);
                let amount = text_combo(&labels, *which);
                hbox.pack_start(&cmp, false, false, 0);
                hbox.pack_start(&amount, false, false, 0);
                let changed = Rc::new(changed);
                let c2 = changed.clone();
                cmp.connect_changed(move |_| c2());
                amount.connect_changed(move |_| changed());
                (hbox.upcast(), false)
            }
        };
        row.hbox.pack_start(&widget, expand, expand, 0);
        widget.show_all();
        *row.widget.borrow_mut() = widget;
    }

    /// The file type menu: Any, Caja's groups, and Other Type..., which
    /// asks for any type the system knows.
    fn type_combo(&self, t: &FileType, changed: impl Fn() + 'static) -> gtk::ComboBox {
        // Shown text, group (-1: none), MIME type of another type, "Other Type...".
        let store = gtk::ListStore::new(&[glib::Type::STRING, glib::Type::I32, glib::Type::STRING, glib::Type::BOOL]);
        let add = |text: &str, group: i32, mime: &str, other: bool| {
            store.insert_with_values(None, &[(0, &text), (1, &group), (2, &mime), (3, &other)])
        };
        add(&tr("Any"), -1, "", false);
        add("---", -1, "", false);
        for (i, (name, _)) in GROUPS.iter().enumerate() {
            add(&tr(name), i as i32, "", false);
        }
        add("---", -1, "", false);
        add(&tr("Other Type…"), -1, "", true);
        let combo = gtk::ComboBox::with_model(&store);
        let cell = gtk::CellRendererText::new();
        combo.pack_start(&cell, true);
        combo.add_attribute(&cell, "text", 0);
        combo.set_row_separator_func(|m, it| m.gv::<String>(it, 0) == "---");
        match t {
            FileType::Any => combo.set_active(Some(0)),
            FileType::Group(g) => combo.set_active(Some(*g as u32 + 2)),
            FileType::Other(mime, desc) => {
                let it = add(desc, -1, mime, false);
                combo.set_active_iter(Some(&it));
            }
        }
        combo.connect_changed(move |combo| {
            let (Some(it), Some(m)) = (combo.active_iter(), combo.model()) else { return };
            if m.gv::<bool>(&it, 3) {
                match pick_type(combo) {
                    Some((mime, desc)) => {
                        let store = m.downcast_ref::<gtk::ListStore>().expect("the type menu's own store");
                        let it = store.insert_with_values(None, &[(0, &desc), (1, &-1i32), (2, &mime), (3, &false)]);
                        combo.set_active_iter(Some(&it));
                    }
                    None => combo.set_active(Some(0)),
                }
                return;
            }
            changed();
        });
        combo
    }

    /// The criteria the rows are set to.
    pub fn criteria(&self) -> Vec<Criterion> {
        self.rows.borrow().iter().filter_map(|r| read_row(r)).collect()
    }
}

fn read_row(row: &Row) -> Option<Criterion> {
    let w = row.widget.borrow().clone();
    let pair = |w: &gtk::Widget| -> Option<(bool, usize)> {
        let combos: Vec<gtk::ComboBox> = w.downcast_ref::<gtk::Box>()?.children().into_iter().filter_map(|c| c.downcast().ok()).collect();
        Some((combos.first()?.active()? == 1, combos.get(1)?.active()? as usize))
    };
    Some(match row.kind.get() {
        0 => {
            // The chosen folder (just after it's set, only the shown one),
            // and whether its subfolders too.
            let parts = w.downcast_ref::<gtk::Box>()?.children();
            let chooser = parts.iter().find_map(|c| c.downcast_ref::<gtk::FileChooserButton>().cloned())?;
            let sub = parts.iter().find_map(|c| c.downcast_ref::<gtk::CheckButton>().cloned())?;
            let uri = chooser.uri().or_else(|| chooser.current_folder_uri())?.to_string();
            Criterion::Location { uri, subfolders: sub.is_active() }
        }
        1 => {
            let combo = w.downcast_ref::<gtk::ComboBox>()?;
            let (it, m) = (combo.active_iter()?, combo.model()?);
            let (group, mime) = (m.gv::<i32>(&it, 1), m.gv::<String>(&it, 2));
            Criterion::FileType(if group >= 0 {
                FileType::Group(group as usize)
            } else if !mime.is_empty() {
                FileType::Other(mime, m.gv::<String>(&it, 0))
            } else {
                FileType::Any
            })
        }
        2 => Criterion::Tags(w.downcast_ref::<gtk::Entry>()?.text().to_string()),
        3 => {
            let (greater, duration) = pair(&w)?;
            Criterion::ModTime { greater, duration }
        }
        4 => {
            let (greater, size) = pair(&w)?;
            Criterion::Size { greater, size }
        }
        _ => Criterion::ContainedText(w.downcast_ref::<gtk::Entry>()?.text().to_string()),
    })
}

/// Caja's "Select type" dialog: every type the system knows, by its
/// description. Returns the chosen (MIME type, description).
fn pick_type(parent: &impl IsA<gtk::Widget>) -> Option<(String, String)> {
    let store = gtk::ListStore::new(&[glib::Type::STRING, glib::Type::STRING]);
    for t in gio::content_types_get_registered() {
        let desc = gio::content_type_get_description(&t);
        store.insert_with_values(None, &[(0, &desc.as_str()), (1, &t.as_str())]);
    }
    store.set_sort_column_id(gtk::SortColumn::Index(0), gtk::SortType::Ascending);
    let dialog = gtk::Dialog::new();
    dialog.set_title(&tr("Select type"));
    if let Some(top) = parent.toplevel().and_then(|t| t.downcast::<gtk::Window>().ok()) {
        dialog.set_transient_for(Some(&top));
    }
    dialog.add_button(&tr("_OK"), gtk::ResponseType::Ok);
    dialog.set_default_size(400, 600);
    let scrolled = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    scrolled.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scrolled.set_shadow_type(gtk::ShadowType::In);
    scrolled.set_overlay_scrolling(false);
    dialog.content_area().pack_start(&scrolled, true, true, 6);
    let view = gtk::TreeView::with_model(&store);
    view.selection().set_mode(gtk::SelectionMode::Browse);
    let cell = gtk::CellRendererText::new();
    let col = gtk::TreeViewColumn::new();
    col.set_title(&tr("Name"));
    gtk::prelude::CellLayoutExt::pack_start(&col, &cell, true);
    gtk::prelude::CellLayoutExt::add_attribute(&col, &cell, "text", 0);
    view.append_column(&col);
    view.set_headers_visible(false);
    scrolled.add(&view);
    dialog.show_all();
    let res = dialog.run();
    let picked = (res == gtk::ResponseType::Ok)
        .then(|| view.selection().selected())
        .flatten()
        .map(|(m, it)| (m.gv::<String>(&it, 1), m.gv::<String>(&it, 0)));
    // SAFETY: the dialog is ours and nothing else holds on to it.
    unsafe { dialog.destroy() };
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        fold(text).split(' ').map(str::to_string).collect()
    }

    #[test]
    fn names_need_every_word_in_any_case() {
        assert!(name_matches(&words("un fold"), "Untitled Folder"));
        assert!(name_matches(&words("FOLDER"), "Untitled Folder (2)"));
        assert!(!name_matches(&words("un doc"), "Untitled Folder"));
        // Accents compare decomposed, as in Caja: "é" typed finds "é" either way.
        assert!(name_matches(&words("café"), "Cafe\u{301} Menu.txt"));
        assert!(name_matches(&words("ÄRGER"), "ärger.txt"));
    }

    #[test]
    fn tags_are_unescaped_and_folded() {
        assert_eq!(tags_of("work,Urgent"), ["work", "urgent"]);
        assert_eq!(tags_of("caf\\xc3\\xa9"), [fold("café")]);
    }

    #[test]
    fn text_is_found_across_pieces() {
        let t = crate::testutil::TestDir::new("pieces-text");
        let mut data = vec![b'a'; (1 << 20) - 3];
        data.extend_from_slice("XYÄßQ".as_bytes());
        data.extend(vec![b'b'; 100]);
        let p = t.0.join("big.txt");
        std::fs::write(&p, &data).unwrap();
        assert!(file_contains(&p, &fold("yäßq")));
        assert!(!file_contains(&p, &fold("yäßz")));
    }

    #[test]
    fn without_subfolders_only_the_folder_itself() {
        let t = crate::testutil::TestDir::new("search-subfolders");
        for p in ["invoice-1.pdf", "notes.txt", "invoices/invoice-2.pdf", "invoices/old/invoice-3.pdf"] {
            let p = t.0.join(p);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "").unwrap();
        }
        let uri = gio::File::for_path(&t.0).uri().to_string();
        let names = |subfolders: bool| {
            let q = Query { text: "invoice".into(), criteria: vec![Criterion::Location { uri: uri.clone(), subfolders }] };
            let mut got = vec![];
            run(&q.params(), || true, |hits| {
                got.extend(hits.into_iter().map(|(_, name)| name));
                true
            });
            got.sort();
            got
        };
        assert_eq!(names(false), ["invoice-1.pdf", "invoices"], "the folder's own items, a folder among them, nothing inside it");
        assert_eq!(names(true), ["invoice-1.pdf", "invoice-2.pdf", "invoice-3.pdf", "invoices"]);
    }

    #[test]
    fn criteria_make_the_parameters() {
        let q = Query {
            text: "Un".into(),
            criteria: vec![
                Criterion::Location { uri: "file:///tmp".into(), subfolders: false },
                Criterion::FileType(FileType::Group(8)),
                Criterion::FileType(FileType::Other("image/png".into(), "PNG image".into())),
                Criterion::Tags("a  B".into()),
                Criterion::Size { greater: true, size: 3 },
                Criterion::ContainedText("x".into()),
            ],
        };
        let p = q.params();
        assert_eq!(p.root().uri(), "file:///tmp");
        assert!(!p.subfolders);
        assert_eq!(p.words, ["un"]);
        assert_eq!(p.mimes, ["text/plain", "image/png"]);
        assert_eq!(p.tags, ["a", "b"]);
        assert_eq!(p.size.abs(), sizes()[3].0);
        assert!(p.size > 0);
        assert_eq!(p.text.as_deref(), Some("x"));
        let everywhere = Query { text: "a".into(), criteria: vec![] }.params();
        assert_eq!(everywhere.root().uri(), "file:///");
        assert!(everywhere.subfolders, "no location row: the whole file system, all of it");
        assert_eq!(q.title(), "Search for “Un”");
    }

    #[test]
    fn modification_time_counts_back_from_now() {
        let at_most_a_day = Query { text: String::new(), criteria: vec![Criterion::ModTime { greater: false, duration: 1 }] }.params();
        let now = glib::DateTime::now_local().unwrap().to_unix();
        assert!((-at_most_a_day.timestamp - (now - 86_400)).abs() <= 3601, "{}", at_most_a_day.timestamp);
    }

    #[test]
    fn finds_by_name_type_size_and_text() {
        let t = crate::testutil::TestDir::new("search");
        let root = t.0.as_path();
        std::fs::create_dir_all(root.join("Sub/Deeper")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("Notes.txt"), "Hello World").unwrap();
        std::fs::write(root.join("Sub/notes-old.txt"), "nothing").unwrap();
        std::fs::write(root.join("Sub/Deeper/NOTES.md"), "hello again").unwrap();
        std::fs::write(root.join(".hidden/notes.txt"), "hello").unwrap();
        std::fs::write(root.join("big.bin"), vec![0u8; 20_000]).unwrap();
        let search = |q: Query| {
            let mut out = vec![];
            run(&q.params(), || true, |b| {
                out.extend(b.into_iter().map(|(_, n)| n));
                true
            });
            out.sort();
            out
        };
        let here = gio::File::for_path(root);
        assert_eq!(search(Query::new("notes", &here)), ["NOTES.md", "Notes.txt", "notes-old.txt"]);
        let mut q = Query::new("notes", &here);
        q.criteria.push(Criterion::ContainedText("HELLO".into()));
        assert_eq!(search(q), ["NOTES.md", "Notes.txt"]);
        let mut q = Query::new("", &here);
        q.criteria.push(Criterion::Size { greater: true, size: 0 });
        assert_eq!(search(q), ["big.bin"]);
        let mut q = Query::new("notes", &here);
        q.criteria.push(Criterion::FileType(FileType::Group(8)));
        assert_eq!(search(q), ["Notes.txt", "notes-old.txt"]);
    }

    #[test]
    fn random_names_find_themselves() {
        crate::testutil::cases(3000, |r| {
            let name = r.text(16);
            let words: Vec<String> = fold(&name).split(' ').filter(|w| !w.is_empty()).map(str::to_string).collect();
            assert!(name_matches(&words, &name), "{name:?} with {words:?}");
            let _ = tags_of(&r.text(16));
        });
    }
}
