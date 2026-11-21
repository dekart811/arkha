//! Small helpers: formatting, naming, dialogs, launching.

use crate::i18n::{fill, tr};
use gio::prelude::*;
use gtk::prelude::*;

/// Sizes in IEC units (KiB, MiB: powers of 1024) instead of SI (kB, MB).
/// A process-wide flag, as file operations format sizes on their own thread.
static IEC_UNITS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// GVfs tells its metadata store about every local file removed or moved
/// (GDaemonVfs's local_file_removed / local_file_moved, called by
/// g_file_delete and the like) and waits for the answer. Even decoding an
/// image does it: gdk-pixbuf's glycin loader removes a temporary file. So
/// when the store was busy, GTK loading an icon froze the window until it
/// answered (up to 25 s, D-Bus's limit). From the main thread, the store is
/// now told from a thread of its own, in the same order; other threads (file
/// operations) wait for it as before.
pub fn tell_metadata_store_in_background() {
    use std::ffi::{c_char, CStr, CString};
    use std::sync::{mpsc, OnceLock};
    type Removed = unsafe extern "C" fn(*mut gio::ffi::GVfs, *const c_char);
    type Moved = unsafe extern "C" fn(*mut gio::ffi::GVfs, *const c_char, *const c_char);
    enum Job {
        Removed(usize, CString),
        Moved(usize, CString, CString),
    }
    static ORIGINAL: OnceLock<(Option<Removed>, Option<Moved>)> = OnceLock::new();
    static QUEUE: OnceLock<mpsc::Sender<Job>> = OnceLock::new();

    fn on_main_thread() -> bool {
        glib::MainContext::default().is_owner()
    }
    unsafe extern "C" fn removed(vfs: *mut gio::ffi::GVfs, name: *const c_char) {
        let Some(original) = ORIGINAL.get().and_then(|o| o.0) else { return };
        if on_main_thread() && !name.is_null() {
            if let Some(q) = QUEUE.get() {
                if q.send(Job::Removed(vfs as usize, CStr::from_ptr(name).to_owned())).is_ok() {
                    return;
                }
            }
        }
        original(vfs, name);
    }
    unsafe extern "C" fn moved(vfs: *mut gio::ffi::GVfs, from: *const c_char, to: *const c_char) {
        let Some(original) = ORIGINAL.get().and_then(|o| o.1) else { return };
        if on_main_thread() && !from.is_null() && !to.is_null() {
            if let Some(q) = QUEUE.get() {
                if q.send(Job::Moved(vfs as usize, CStr::from_ptr(from).to_owned(), CStr::from_ptr(to).to_owned())).is_ok() {
                    return;
                }
            }
        }
        original(vfs, from, to);
    }

    if ORIGINAL.get().is_some() {
        return;
    }
    let vfs = gio::Vfs::default();
    // SAFETY: the default GVfs lives as long as the program; its class
    // structure is GVfsClass (or a subclass of it), and only the two
    // functions are swapped, before anything else runs on other threads.
    unsafe {
        use glib::translate::ToGlibPtr;
        let instance: *mut gio::ffi::GVfs = vfs.to_glib_none().0;
        let class = (*(instance as *mut glib::gobject_ffi::GTypeInstance)).g_class as *mut gio::ffi::GVfsClass;
        let original = ((*class).local_file_removed, (*class).local_file_moved);
        if original == (None, None) || ORIGINAL.set(original).is_err() {
            return;
        }
        let (tx, rx) = mpsc::channel::<Job>();
        let started = std::thread::Builder::new().name("metadata".into()).spawn(move || {
            let (removed, moved) = original;
            for job in rx {
                match job {
                    Job::Removed(vfs, name) => {
                        if let Some(f) = removed {
                            f(vfs as *mut gio::ffi::GVfs, name.as_ptr());
                        }
                    }
                    Job::Moved(vfs, from, to) => {
                        if let Some(f) = moved {
                            f(vfs as *mut gio::ffi::GVfs, from.as_ptr(), to.as_ptr());
                        }
                    }
                }
            }
        });
        if started.is_err() || QUEUE.set(tx).is_err() {
            return;
        }
        if original.0.is_some() {
            (*class).local_file_removed = Some(removed);
        }
        if original.1.is_some() {
            (*class).local_file_moved = Some(moved);
        }
    }
}

/// GtkTreeView::scroll_to_cell, once the view is realized. Asked before
/// that, GTK keeps the scroll for its next size allocation, which can come
/// while the view still isn't (a column of a tab not shown yet), and then
/// warns (gtk_tree_view_get_cell_area): it's asked again when the view
/// shows.
pub fn scroll_tree_to(view: &gtk::TreeView, path: &gtk::TreePath, use_align: bool, row_align: f32, col_align: f32) {
    when_shown(view, path, move |v, p| v.scroll_to_cell(Some(p), None::<&gtk::TreeViewColumn>, use_align, row_align, col_align));
}

/// Moves a list view's cursor to `path`, as scroll_tree_to scrolls (the
/// cursor scrolls to its row: GTK would do that at the wrong moment too).
/// The selection stays as it is (GTK selects the cursor's row).
pub fn set_tree_cursor(view: &gtk::TreeView, path: &gtk::TreePath) {
    when_shown(view, path, |v, p| {
        let sel = v.selection();
        let (rows, _) = sel.selected_rows();
        v.set_cursor(p, None::<&gtk::TreeViewColumn>, false);
        sel.unselect_all();
        for r in &rows {
            sel.select_path(r);
        }
    });
}

/// Runs `f` on `view` and `path` now if the view is realized, else once it
/// shows; either way only if it then has a model with that row (by then
/// its tab may have gone to icons, with the model on the icon view).
fn when_shown(view: &gtk::TreeView, path: &gtk::TreePath, f: impl Fn(&gtk::TreeView, &gtk::TreePath) + 'static) {
    let has_row = |v: &gtk::TreeView, p: &gtk::TreePath| v.model().is_some_and(|m| m.iter(p).is_some());
    if view.is_realized() {
        if has_row(view, path) {
            f(view, path);
        }
        return;
    }
    let path = path.clone();
    let handler: std::rc::Rc<std::cell::RefCell<Option<glib::SignalHandlerId>>> = Default::default();
    let h = handler.clone();
    let id = view.connect_map(move |v| {
        if let Some(id) = h.borrow_mut().take() {
            v.disconnect(id);
        }
        if has_row(v, &path) {
            f(v, &path);
        }
    });
    *handler.borrow_mut() = Some(id);
}

/// Hands memory freed in bulk (a closed tab, a big folder left) back to
/// the system, a moment later and once for several. Folders are many small
/// pieces (a row, its texts), and the allocator keeps the pages they leave
/// for later: Arkha would go on showing the most it ever used.
pub fn release_memory_soon() {
    thread_local! { static PENDING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    if PENDING.with(|p| p.replace(true)) {
        return;
    }
    glib::timeout_add_local_once(std::time::Duration::from_secs(1), || {
        PENDING.with(|p| p.set(false));
        #[cfg(target_env = "gnu")]
        // SAFETY: no arguments to get wrong; glibc's own locking.
        unsafe {
            libc::malloc_trim(0);
        }
    });
}

/// How format_time shows dates: 0 informal, 1 the locale's way, 2 ISO.
static DATE_FORMAT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// "informal" (Today 14:00, else 7 Nov 2026 14:00), "locale" (as the
/// language has it) or "iso" (2026-11-07 14:00:00), as in Caja.
pub fn set_date_format(name: &str) {
    let v = match name {
        "locale" => 1,
        "iso" => 2,
        _ => 0,
    };
    DATE_FORMAT.store(v, std::sync::atomic::Ordering::Relaxed);
}

pub fn set_iec_units(on: bool) {
    IEC_UNITS.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn format_size(n: u64) -> String {
    let flags = if IEC_UNITS.load(std::sync::atomic::Ordering::Relaxed) { glib::FormatSizeFlags::IEC_UNITS } else { glib::FormatSizeFlags::DEFAULT };
    glib::format_size_full(n, flags).to_string()
}

/// Writes a settings file: whole or not at all (a new file renamed over
/// the old), but without waiting for the disk to have it, as
/// KeyFile::save_to_file does. That takes a moment, longer while the disk
/// is busy (with a copy, say), and settings are saved as they change.
/// A settings file, read. GLib refuses a whole file over one line it
/// can't read (typed by hand: a line without "=", a broken "[group"), and
/// Arkha then started with its defaults and saved them over it: all
/// settings gone. Such lines are now left out and the rest read; the file
/// as it was is kept beside it as NAME.broken, which the output mentions.
/// None if there's no file, or nothing in it can be read.
pub fn load_settings(path: &std::path::Path) -> Option<glib::KeyFile> {
    let data = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&data);
    let kf = glib::KeyFile::new();
    if kf.load_from_data(&text, glib::KeyFileFlags::NONE).is_ok() {
        return Some(kf);
    }
    let kf = glib::KeyFile::new();
    kf.load_from_data(&readable_lines(&text), glib::KeyFileFlags::NONE).ok()?;
    let mut broken = path.as_os_str().to_os_string();
    broken.push(".broken");
    let _ = std::fs::write(&broken, &data);
    glib::g_message!("arkha", "settings: lines in {} couldn't be read and were left out; the file as it was is in {}", path.display(), std::path::Path::new(&broken).display());
    Some(kf)
}

/// The lines of a settings file GLib can read: comments, empty lines,
/// "[group]" lines, and "key=value" lines in a group.
fn readable_lines(text: &str) -> String {
    let mut out = String::new();
    let mut in_group = false;
    for line in text.lines() {
        let t = line.trim();
        let ok = if t.is_empty() || t.starts_with('#') {
            true
        } else if t.starts_with('[') {
            let group = t.len() > 2 && t.ends_with(']') && !t[1..t.len() - 1].contains(['[', ']']);
            in_group |= group;
            group
        } else {
            in_group && t.split_once('=').is_some_and(|(k, _)| !k.trim().is_empty())
        };
        if ok {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

pub fn save_settings(kf: &glib::KeyFile, path: &std::path::Path) {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    // GLib's own way (file_set_contents_full, CONSISTENT) flushes too.
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    if std::fs::write(&tmp, kf.to_data().as_bytes()).is_err() || std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Arkha's own icon (`arkha`, installed in the hicolor theme), or the icon
/// theme's file manager icon where it isn't installed, as when run from the
/// build folder.
pub fn app_icon() -> &'static str {
    static ICON: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    ICON.get_or_init(|| {
        let ours = gtk::IconTheme::default().is_some_and(|t| t.has_icon("arkha"));
        if ours { "arkha" } else { "system-file-manager" }
    })
}

/// Shows a desktop notification from Arkha (as libnotify does, which Caja
/// uses: clicking it does nothing).
pub fn notify(summary: &str, body: &str) {
    send_notification(&Notification { summary, body, icon: app_icon(), sticky: false }, |_| {});
}

pub struct Notification<'a> {
    pub summary: &'a str,
    pub body: &'a str,
    pub icon: &'a str,
    /// Stays until it's closed (`close_notification`) instead of timing
    /// out, and isn't kept in the notification history (critical urgency,
    /// transient), like Caja's "Writing data to …".
    pub sticky: bool,
}

/// Sends `n`; `shown` gets its id (0 if it couldn't be shown).
pub fn send_notification(n: &Notification, shown: impl FnOnce(u32) + 'static) {
    let mut hints: std::collections::HashMap<String, glib::Variant> = [("desktop-entry".to_string(), "arkha".to_variant())].into();
    if n.sticky {
        hints.insert("urgency".to_string(), 2u8.to_variant());
        hints.insert("transient".to_string(), true.to_variant());
    }
    let args = ("Arkha", 0u32, n.icon, n.summary, n.body, Vec::<String>::new(), hints, -1i32).to_variant();
    notifications_call("Notify", args, move |reply| shown(reply.and_then(|r| r.child_value(0).get::<u32>()).unwrap_or(0)));
}

pub fn close_notification(id: u32) {
    if id != 0 {
        notifications_call("CloseNotification", (id,).to_variant(), |_| {});
    }
}

fn notifications_call(method: &'static str, args: glib::Variant, done: impl FnOnce(Option<glib::Variant>) + 'static) {
    gio::bus_get(gio::BusType::Session, gio::Cancellable::NONE, move |bus| {
        let Ok(bus) = bus else { return done(None) };
        bus.call(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            method,
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
            move |r| done(r.ok()),
        );
    });
}

pub fn format_time(t: i64) -> String {
    if t <= 0 {
        return String::new();
    }
    let Some(dt) = local_time(t) else {
        return String::new();
    };
    match DATE_FORMAT.load(std::sync::atomic::Ordering::Relaxed) {
        1 => return dt.format("%c").map(|s| s.trim().to_string()).unwrap_or_default(),
        2 => return dt.format("%Y-%m-%d %H:%M:%S").map(|s| s.to_string()).unwrap_or_default(),
        _ => {}
    }
    let Some((today, yesterday)) = recent_days() else {
        return String::new();
    };
    thread_local! {
        // Looked up once: a folder's listing formats thousands of dates.
        static FORMATS: [String; 3] = [
            // TRANSLATORS: a time today, in strftime's notation (%H hours, %M minutes).
            tr("Today %H:%M"),
            // TRANSLATORS: a time yesterday, in strftime's notation.
            tr("Yesterday %H:%M"),
            // TRANSLATORS: a date and time in strftime's notation: %-d the day,
            // %b the month's short name, %Y the year. Order it as your language does.
            tr("%-d %b %Y %H:%M"),
        ];
    }
    let i = if dt.ymd() == today {
        0
    } else if dt.ymd() == yesterday {
        1
    } else {
        2
    };
    FORMATS.with(|f| dt.format(&f[i]).map(|s| s.trim().to_string()).unwrap_or_default())
}

/// (year, month, day)
type Date = (i32, i32, i32);

/// The local time zone, looked up again once a minute: GLib reads
/// /etc/localtime's details from the disk for each lookup, and a folder's
/// listing formats thousands of dates.
fn local_zone() -> glib::TimeZone {
    type Zone = Option<(u64, glib::TimeZone)>;
    thread_local! {
        static ZONE: std::cell::RefCell<Zone> = const { std::cell::RefCell::new(None) };
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    ZONE.with(|z| {
        let mut z = z.borrow_mut();
        if let Some((at, zone)) = &*z {
            if (*at..*at + 60).contains(&now) {
                return zone.clone();
            }
        }
        let zone = glib::TimeZone::local();
        *z = Some((now, zone.clone()));
        zone
    })
}

/// `t` (seconds since 1970) in the local time zone.
fn local_time(t: i64) -> Option<glib::DateTime> {
    glib::DateTime::from_unix_utc(t).ok()?.to_timezone(&local_zone()).ok()
}

/// Today's and yesterday's dates, looked up once a minute: a folder's
/// listing formats thousands of times.
fn recent_days() -> Option<(Date, Date)> {
    type Days = Option<(i64, Date, Date)>;
    thread_local! {
        static DAYS: std::cell::Cell<Days> = const { std::cell::Cell::new(None) };
    }
    let now = glib::DateTime::now(&local_zone()).ok()?;
    if let Some((at, today, yesterday)) = DAYS.with(|d| d.get()) {
        if now.to_unix() - at < 60 && now.ymd() == today {
            return Some((today, yesterday));
        }
    }
    let days = (now.ymd(), now.add_days(-1).ok()?.ymd());
    DAYS.with(|d| d.set(Some((now.to_unix(), days.0, days.1))));
    Some(days)
}

pub fn format_time_long(t: i64) -> String {
    local_time(t)
        // TRANSLATORS: a full date and time in strftime's notation: %A the
        // weekday, %-d the day, %B the month, %Y the year.
        .and_then(|d| d.format(&tr("%A, %-d %B %Y, %H:%M:%S")).ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| tr("Unknown"))
}

/// Split "archive.tar.gz" into ("archive", ".tar.gz"), "photo.jpg" into ("photo", ".jpg").
/// Hidden files without another dot have no extension.
pub fn split_ext(name: &str) -> (&str, &str) {
    let lower = name.to_lowercase();
    for multi in [".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst", ".tar.lz"] {
        if lower.ends_with(multi) && name.len() > multi.len() {
            let i = name.len() - multi.len();
            return (&name[..i], &name[i..]);
        }
    }
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// The name `make` makes of `stem` (adding " (copy)", " (2)"…), with the
/// stem cut short where the whole would be longer than names can be (255
/// bytes): with a long name, a copy or a duplicate couldn't be made.
fn fit_name(stem: &str, make: impl Fn(&str) -> String) -> String {
    let mut stem = stem.to_string();
    loop {
        let name = make(&stem);
        if name.len() <= 255 || stem.is_empty() {
            return name;
        }
        stem.pop();
    }
}

/// A name in `dir` that doesn't exist yet: "name", "name (2)", ...
pub fn unique_name(dir: &gio::File, name: &str, is_dir: bool) -> String {
    if !dir.child(name).query_exists(gio::Cancellable::NONE) {
        return name.to_string();
    }
    let (stem, ext) = if is_dir { (name, "") } else { split_ext(name) };
    for i in 2.. {
        // TRANSLATORS: the name for another item of the same name: "photo (2).jpg".
        let cand = fit_name(stem, |stem| fill(&tr("{name} ({number}){extension}"), &[("name", &stem), ("number", &i), ("extension", &ext)]));
        if !dir.child(&cand).query_exists(gio::Cancellable::NONE) {
            return cand;
        }
    }
    unreachable!()
}

/// Name for a copy pasted into the same folder: "a (copy).txt", "a (copy 2).txt", ...
pub fn copy_name(dir: &gio::File, name: &str, is_dir: bool) -> String {
    let (stem, ext) = if is_dir { (name, "") } else { split_ext(name) };
    // TRANSLATORS: the name of a copy pasted beside its original: "photo (copy).jpg".
    let first = fit_name(stem, |stem| fill(&tr("{name} (copy){extension}"), &[("name", &stem), ("extension", &ext)]));
    if !dir.child(&first).query_exists(gio::Cancellable::NONE) {
        return first;
    }
    for i in 2.. {
        // TRANSLATORS: the name of a further copy: "photo (copy 2).jpg".
        let cand = fit_name(stem, |stem| fill(&tr("{name} (copy {number}){extension}"), &[("name", &stem), ("number", &i), ("extension", &ext)]));
        if !dir.child(&cand).query_exists(gio::Cancellable::NONE) {
            return cand;
        }
    }
    unreachable!()
}

pub fn display_name(file: &gio::File) -> String {
    if let Ok(info) = file.query_info(
        "standard::display-name",
        gio::FileQueryInfoFlags::NONE,
        gio::Cancellable::NONE,
    ) {
        return info.display_name().to_string();
    }
    file.basename()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| file.uri().to_string())
}

/// Human readable location for the title / properties.
pub fn pretty_location(file: &gio::File) -> String {
    match file.path() {
        Some(p) => {
            let s = p.to_string_lossy().to_string();
            if let Some(home) = dirs_home() {
                if s == home {
                    return "~".into();
                }
            }
            s
        }
        None => glib::Uri::unescape_string(file.uri().as_str(), None::<&str>)
            .map(|s| s.to_string())
            .unwrap_or_else(|| file.uri().to_string()),
    }
}

pub fn dirs_home() -> Option<String> {
    Some(glib::home_dir().to_string_lossy().to_string())
}

pub fn home_file() -> gio::File {
    gio::File::for_path(glib::home_dir())
}

pub fn error_dialog(parent: Option<&gtk::Window>, title: &str, detail: &str) {
    let d = gtk::MessageDialog::new(
        parent,
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Error,
        gtk::ButtonsType::Close,
        title,
    );
    d.set_secondary_text(Some(detail));
    d.connect_response(|d, _| d.close());
    d.show();
}

/// Find the first program on $PATH out of the candidates.
pub fn find_program(cands: &[&str]) -> Option<String> {
    cands
        .iter()
        .find(|c| glib::find_program_in_path(c).is_some())
        .map(|s| s.to_string())
}

/// Runs `argv` in `dir`. Its words as bytes (OsStr): a file's name needn't
/// be UTF-8, and made into text it was another name.
pub fn spawn_in_dir<S: AsRef<std::ffi::OsStr>>(argv: &[S], dir: Option<&std::path::Path>) -> Result<(), String> {
    let Some(program) = argv.first() else { return Err("nothing to run".into()) };
    let mut cmd = std::process::Command::new(program);
    cmd.args(&argv[1..]);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    // Reap it when it exits so it doesn't linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// argv to run `program` in a new window of `term`, keeping the path as one argument.
pub fn terminal_run_argv(term: &str, program: &std::ffi::OsStr) -> Vec<std::ffi::OsString> {
    let name = std::path::Path::new(term)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut argv: Vec<std::ffi::OsString> = vec![term.into()];
    match name.as_str() {
        "gnome-terminal" => argv.push("--".into()),
        "mate-terminal" | "xfce4-terminal" => argv.push("-x".into()),
        "kitty" => {}
        _ => argv.push("-e".into()),
    }
    argv.push(program.into());
    argv
}

pub fn terminal_command() -> Option<String> {
    if let Ok(t) = std::env::var("TERMINAL") {
        if glib::find_program_in_path(&t).is_some() {
            return Some(t);
        }
    }
    find_program(&[
        "mate-terminal",
        "x-terminal-emulator",
        "xfce4-terminal",
        "gnome-terminal",
        "tilix",
        "konsole",
        "alacritty",
        "kitty",
        "xterm",
    ])
}

/// The archive manager for Extract Here and Compress: the one chosen in
/// Preferences, if it's installed; else ("Automatic") the one set as the
/// application for archives (zip), if it's one of these, else the first
/// installed, KDE's Ark first on KDE.
pub fn archiver_command() -> Option<String> {
    let chosen = crate::config::with(|s| s.archiver.clone());
    if ARCHIVERS.contains(&chosen.as_str()) && glib::find_program_in_path(&chosen).is_some() {
        return Some(chosen);
    }
    let default = gio::AppInfo::default_for_type("application/zip", false)
        .map(|a| a.executable())
        .and_then(|e| e.file_name().map(|n| n.to_string_lossy().into_owned()));
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    choose_archiver(default.as_deref(), &desktop, |p| glib::find_program_in_path(p).is_some()).map(str::to_string)
}

/// The archive managers Arkha can use, by command.
pub const ARCHIVERS: [&str; 4] = ["engrampa", "file-roller", "xarchiver", "ark"];

/// An archive manager's name, for Preferences.
pub fn archiver_name(command: &str) -> &str {
    match command {
        "engrampa" => "Engrampa",
        "file-roller" => "File Roller",
        "xarchiver" => "Xarchiver",
        "ark" => "Ark",
        other => other,
    }
}

fn choose_archiver(default: Option<&str>, desktop: &str, installed: impl Fn(&str) -> bool) -> Option<&'static str> {
    if let Some(d) = default.and_then(|d| ARCHIVERS.iter().find(|a| **a == d)) {
        if installed(d) {
            return Some(d);
        }
    }
    let kde = desktop.split(':').any(|d| d == "KDE");
    let order = if kde { ["ark", "engrampa", "file-roller", "xarchiver"] } else { ARCHIVERS };
    order.into_iter().find(|a| installed(a))
}

/// The command line for Extract Here (`extract`) or Compress with `tool`
/// (as archiver_command found it) on `paths`.
pub fn archive_argv(tool: &str, extract: bool, paths: Vec<std::ffi::OsString>) -> Vec<std::ffi::OsString> {
    let flags: &[&str] = match (tool, extract) {
        // Next to the archive, in a folder of its name unless it holds just
        // one file or folder, as the others' --extract-here.
        ("ark", true) => &["--batch", "--autodestination", "--autosubfolder"],
        // Asks for the archive's name and type in Ark's dialog (without
        // --dialog, --add wants a name or --autofilename).
        ("ark", false) => &["--changetofirstpath", "--add", "--dialog"],
        (_, true) => &["--extract-here"],
        ("xarchiver", false) => &["--compress"],
        (_, false) => &["--add"],
    };
    std::iter::once(tool.into()).chain(flags.iter().map(|f| f.into())).chain(paths).collect()
}

pub fn is_archive(ctype: &str) -> bool {
    const T: &[&str] = &[
        "application/zip",
        "application/x-tar",
        "application/x-compressed-tar",
        "application/x-bzip-compressed-tar",
        "application/x-xz-compressed-tar",
        "application/x-zstd-compressed-tar",
        "application/x-7z-compressed",
        "application/vnd.rar",
        "application/x-rar",
        "application/gzip",
        "application/x-bzip",
        "application/x-xz",
        "application/x-lzma",
        "application/x-cpio",
        "application/x-rpm",
        "application/vnd.debian.binary-package",
        "application/x-deb",
        "application/x-iso9660-image",
        "application/java-archive",
    ];
    T.contains(&ctype)
}

// Some backends (phones over MTP) leave attributes out of a GFileInfo, and
// its getters warn about that ("GFileInfo created without …"). These check
// first and fall back on the name.

/// The item's name, if the info has it.
fn info_name(info: &gio::FileInfo) -> Option<String> {
    info.has_attribute("standard::name").then(|| info.name().to_string_lossy().to_string())
}

/// Hidden: as reported, or else a name starting with a dot.
pub fn info_is_hidden(info: &gio::FileInfo) -> bool {
    if info.has_attribute("standard::is-hidden") {
        return info.is_hidden();
    }
    info_name(info).is_some_and(|n| n.starts_with('.'))
}

/// Whether an item named `name` is left out while hidden items are: a dot
/// file or a backup ("~"), going by the name alone.
pub fn name_hidden(name: &str) -> bool {
    name.starts_with('.') || name.ends_with('~')
}

/// Backup file: as reported, or else a name ending in "~".
pub fn info_is_backup(info: &gio::FileInfo) -> bool {
    if info.has_attribute("standard::is-backup") {
        return info.is_backup();
    }
    info_name(info).is_some_and(|n| n.ends_with('~'))
}

pub fn info_is_symlink(info: &gio::FileInfo) -> bool {
    info.has_attribute("standard::is-symlink") && info.is_symlink()
}

/// The content type: as reported, or else guessed from the name.
pub fn info_content_type(info: &gio::FileInfo) -> Option<String> {
    if info.has_attribute("standard::content-type") {
        return info.content_type().map(|s| s.to_string());
    }
    info_name(info).map(|n| guess_content_type(&n))
}

/// The content type for a file name. (gio-rs passes an empty slice as
/// zero bytes of data, which makes every guess "application/x-zerosize";
/// no data at all has to be NULL.)
fn guess_content_type(name: &str) -> String {
    use glib::translate::*;
    unsafe {
        let p = gio::ffi::g_content_type_guess(name.to_glib_none().0, std::ptr::null(), 0, std::ptr::null_mut());
        glib::GString::from_glib_full(p).to_string()
    }
}

/// Bookmarks are shared with Caja / GTK file chooser.
pub fn bookmarks_file() -> std::path::PathBuf {
    crate::config::config_home().join("gtk-3.0").join("bookmarks")
}

pub fn read_bookmarks() -> Vec<(String, String)> {
    std::fs::read_to_string(bookmarks_file())
        .map(|text| parse_bookmarks(&text))
        .unwrap_or_default()
}

/// (URI, label) of each line of a bookmarks file; without a label, the
/// URI's file name.
fn parse_bookmarks(text: &str) -> Vec<(String, String)> {
    bookmark_lines(text)
        .into_iter()
        .map(|(uri, label)| {
            let label = if label.is_empty() { default_bookmark_label(&uri) } else { label };
            (uri, label)
        })
        .collect()
}

/// (URI, label) of each line as written: the label empty without one.
fn bookmark_lines(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| match l.split_once(' ') {
            Some((u, n)) => (u.to_string(), n.to_string()),
            None => (l.to_string(), String::new()),
        })
        .collect()
}

/// What a bookmark without a label is called: its folder's name. A
/// share's top (smb://server/Share/), which GVfs calls "/", by the last part
/// of its address ("Share"; "server" for smb://server/).
fn default_bookmark_label(uri: &str) -> String {
    match gio::File::for_uri(uri).basename().map(|b| b.to_string_lossy().to_string()) {
        Some(b) if !b.is_empty() && b != "/" => b,
        _ => {
            let unescaped = glib::Uri::unescape_string(uri, None::<&str>).map_or_else(|| uri.to_string(), |u| u.to_string());
            let trimmed = unescaped.trim_end_matches('/');
            trimmed.rsplit('/').next().filter(|p| !p.is_empty()).unwrap_or(trimmed).to_string()
        }
    }
}

/// Changes the bookmarks file: `f` gets its (URI, label) lines, the label
/// empty for the folder's name. A label that's just the folder's name isn't
/// written. The windows watch the file and update their bookmarks.
pub fn edit_bookmarks(f: impl FnOnce(&mut Vec<(String, String)>)) -> std::io::Result<()> {
    edit_bookmarks_at(&bookmarks_file(), f)
}

fn edit_bookmarks_at(path: &std::path::Path, f: impl FnOnce(&mut Vec<(String, String)>)) -> std::io::Result<()> {
    let old = bookmark_lines(&std::fs::read_to_string(path).unwrap_or_default());
    let mut b = old.clone();
    f(&mut b);
    if b == old {
        return Ok(());
    }
    let mut out = String::new();
    for (uri, label) in &b {
        out.push_str(uri);
        if !label.is_empty() && *label != default_bookmark_label(uri) {
            out.push(' ');
            out.push_str(label);
        }
        out.push('\n');
    }
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(path, out)
}

pub fn add_bookmark(uri: &str) -> std::io::Result<bool> {
    add_bookmark_to(&bookmarks_file(), uri)
}

/// Adds `uri` at the end of the bookmarks file `path`; false if it's there
/// already.
fn add_bookmark_to(path: &std::path::Path, uri: &str) -> std::io::Result<bool> {
    let mut added = false;
    edit_bookmarks_at(path, |b| {
        if !b.iter().any(|(u, _)| u == uri) {
            b.push((uri.to_string(), String::new()));
            added = true;
        }
    })?;
    Ok(added)
}

/// Bookmarks `uri` under `name` (Connect to Server's "Add bookmark"): at
/// the end if it isn't there, else it keeps its place and gets the name.
/// An empty name is the folder's name for a new bookmark, and leaves an
/// existing one's label as it is.
pub fn bookmark_named(uri: &str, name: &str) -> std::io::Result<()> {
    bookmark_named_at(&bookmarks_file(), uri, name)
}

fn bookmark_named_at(path: &std::path::Path, uri: &str, name: &str) -> std::io::Result<()> {
    let name = name.trim();
    edit_bookmarks_at(path, |b| match b.iter_mut().find(|(u, _)| u == uri) {
        Some(existing) => {
            if !name.is_empty() {
                existing.1 = name.to_string();
            }
        }
        None => b.push((uri.to_string(), name.to_string())),
    })
}

/// Minimal shell-style glob: * ? and [abc].
pub fn glob_match(pat: &[char], s: &[char]) -> bool {
    // On a mismatch only the last "*" takes one more character: trying every
    // split for every "*" takes exponential time with a few of them.
    let (mut p, mut i) = (0, 0);
    // The pattern position after the last "*", and the text position it's up to.
    let mut star: Option<(usize, usize)> = None;
    while i < s.len() {
        if pat.get(p) == Some(&'*') {
            p += 1;
            star = Some((p, i));
            continue;
        }
        if let Some(len) = pat.get(p..).and_then(|rest| glob_one(rest, s[i])) {
            p += len;
            i += 1;
            continue;
        }
        let Some((sp, si)) = star else { return false };
        p = sp;
        i = si + 1;
        star = Some((sp, i));
    }
    pat[p..].iter().all(|c| *c == '*')
}

/// Whether the first element of `pat` (a character, "?" or a "[set]")
/// matches `c`: how many pattern characters it took, or None.
fn glob_one(pat: &[char], c: char) -> Option<usize> {
    match *pat.first()? {
        '?' => Some(1),
        '[' => match pat.iter().position(|x| *x == ']') {
            Some(end) if end > 1 => pat[1..end].contains(&c).then_some(end + 1),
            _ => (c == '[').then_some(1),
        },
        p => (p == c).then_some(1),
    }
}

/// Whether a text field has the focus (an entry, or the multi-line rename
/// editor of the icon views): its keys (Ctrl+A, Delete, Backspace, …) are
/// then for the text, not the window's shortcuts.
pub fn is_editable_focused(win: &gtk::Window) -> bool {
    win.focused_widget()
        .map(|w| w.is::<gtk::Editable>() || w.is::<gtk::TextView>())
        .unwrap_or(false)
}

/// Puts `nb`'s tabs (each page named by its id) in the order `kept` and
/// lets them be dragged into another; the new order goes to `save`, with
/// the tabs not shown this time (see merge_tab_order). For Rename All and
/// Properties.
pub fn order_tabs(nb: &gtk::Notebook, kept: Vec<String>, save: impl Fn(Vec<String>) + 'static) {
    let pages = || (0..nb.n_pages()).filter_map(|i| nb.nth_page(Some(i))).collect::<Vec<_>>();
    let mut at = 0;
    for id in &kept {
        if let Some(page) = pages().into_iter().find(|p| p.widget_name() == id.as_str()) {
            nb.reorder_child(&page, Some(at));
            at += 1;
        }
    }
    for page in pages() {
        nb.set_tab_reorderable(&page, true);
    }
    nb.set_current_page(Some(0));
    let kept = std::cell::RefCell::new(kept);
    nb.connect_page_reordered(move |nb, _, _| {
        let shown: Vec<String> = (0..nb.n_pages()).filter_map(|i| nb.nth_page(Some(i))).map(|p| p.widget_name().to_string()).collect();
        let order = merge_tab_order(&kept.borrow(), &shown);
        *kept.borrow_mut() = order.clone();
        save(order);
    });
}

/// The tabs shown now in their new order, with those of `kept` not shown
/// this time (Properties' Open With for a folder, say) each after the tab
/// that was before it.
pub fn merge_tab_order(kept: &[String], shown: &[String]) -> Vec<String> {
    let mut order = shown.to_vec();
    for (i, id) in kept.iter().enumerate() {
        if order.contains(id) {
            continue;
        }
        let at = kept[..i].iter().rev().find_map(|p| order.iter().position(|o| o == p)).map_or(0, |p| p + 1);
        order.insert(at, id.clone());
    }
    order
}

/// A key file's groups named `prefix` and a number ("Action 2"), in the
/// numbers' order: the items of a list saved one group each.
pub fn numbered_groups(kf: &glib::KeyFile, prefix: &str) -> Vec<String> {
    let mut groups: Vec<(u32, String)> = kf.groups().iter().filter_map(|g| Some((g.strip_prefix(prefix)?.parse().ok()?, g.to_string()))).collect();
    groups.sort();
    groups.into_iter().map(|(_, g)| g).collect()
}

/// `name` with the characters that change the direction of the text after
/// them shown as "<U+202E>" instead of obeyed: with one, "gpj.exe" shows as
/// "exe.jpg". For the questions about running and deleting files.
pub fn visible_controls(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '\u{200e}' | '\u{200f}' | '\u{061c}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => format!("<U+{:04X}>", c as u32),
            c => c.to_string(),
        })
        .collect()
}

/// `command`, a shell command, with each %x replaced by the words `value`
/// gives for x, each shell-quoted (joined by spaces): "%%" is a % sign,
/// and a %x it has nothing for stays as it is. A placeholder within quotes
/// written in the command ("%f", '%f', "%f.bak") gets the quotes closed
/// around it and opened again after it: quoted twice, a name with $(…) in
/// it ran that inside double quotes.
pub fn expand_shell(command: &str, value: impl Fn(char) -> Option<Vec<std::ffi::OsString>>) -> std::ffi::OsString {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    #[derive(PartialEq, Clone, Copy)]
    enum In {
        Plain,
        Single,
        Double,
    }
    let mut at = In::Plain;
    let mut out: Vec<u8> = vec![];
    let push = |out: &mut Vec<u8>, c: char| out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        match (at, c) {
            (_, '%') => match chars.next() {
                Some('%') => out.push(b'%'),
                Some(x) => match value(x) {
                    Some(words) => {
                        // Words as bytes: a name needn't be UTF-8.
                        let quoted: Vec<Vec<u8>> = words.iter().map(|w| shell_word(w.as_bytes())).collect();
                        let quoted = quoted.join(&b' ');
                        match at {
                            In::Plain => out.extend(quoted),
                            In::Single => {
                                out.push(b'\'');
                                out.extend(quoted);
                                out.push(b'\'');
                            }
                            In::Double => {
                                out.push(b'"');
                                out.extend(quoted);
                                out.push(b'"');
                            }
                        }
                    }
                    None => {
                        out.push(b'%');
                        push(&mut out, x);
                    }
                },
                None => out.push(b'%'),
            },
            // A backslash keeps the next character as it is (outside
            // quotes and in double quotes, not in single ones).
            (In::Plain | In::Double, '\\') => {
                push(&mut out, c);
                if let Some(n) = chars.next() {
                    push(&mut out, n);
                }
            }
            (In::Plain, '\'') => {
                at = In::Single;
                push(&mut out, c);
            }
            (In::Plain, '"') => {
                at = In::Double;
                push(&mut out, c);
            }
            (In::Single, '\'') | (In::Double, '"') => {
                at = In::Plain;
                push(&mut out, c);
            }
            _ => push(&mut out, c),
        }
    }
    std::ffi::OsString::from_vec(out)
}

/// `w` as one word for sh: as it is if it's only harmless characters (a
/// size), else in single quotes ("'" as '\''), as GLib quotes; bytes, as
/// names needn't be UTF-8.
fn shell_word(w: &[u8]) -> Vec<u8> {
    if !w.is_empty() && w.iter().all(|c| c.is_ascii_alphanumeric() || b"_./+,:=@-".contains(c)) {
        return w.to_vec();
    }
    let mut out = vec![b'\''];
    for &c in w {
        if c == b'\'' {
            out.extend_from_slice(b"'\\''");
        } else {
            out.push(c);
        }
    }
    out.push(b'\'');
    out
}

/// `command` with each %x replaced by what `value` gives for x: "%%" is a
/// % sign, and a %x it has nothing for stays as it is.
pub fn expand_placeholders(command: &str, value: impl Fn(char) -> Option<std::ffi::OsString>) -> std::ffi::OsString {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let mut out: Vec<u8> = vec![];
    let push = |out: &mut Vec<u8>, c: char| out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            push(&mut out, c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push(b'%'),
            Some(x) => match value(x) {
                Some(v) => out.extend_from_slice(v.as_bytes()),
                None => {
                    out.push(b'%');
                    push(&mut out, x);
                }
            },
            None => out.push(b'%'),
        }
    }
    std::ffi::OsString::from_vec(out)
}

/// Whether a preview set to `mode` ("always", "local", "never", as Caja's
/// Preview settings) is shown for `file`. Local files are those with a
/// path of their own, network shares mounted by the system included; GVfs
/// places (smb://, sftp://, a phone) aren't.
pub fn preview_on(mode: &str, file: &gio::File) -> bool {
    match mode {
        "always" => true,
        "never" => false,
        // The trash is local too, as in Caja.
        _ => file.is_native() || file.has_uri_scheme("trash"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archiver_set_for_archives_else_the_first_installed() {
        let all = |_: &str| true;
        assert_eq!(choose_archiver(Some("ark"), "MATE", all), Some("ark"), "the one set for archives");
        assert_eq!(choose_archiver(Some("file-roller"), "KDE", all), Some("file-roller"));
        assert_eq!(choose_archiver(None, "MATE", all), Some("engrampa"), "else the first");
        assert_eq!(choose_archiver(Some("peazip"), "KDE", all), Some("ark"), "on KDE Ark first");
        assert_eq!(choose_archiver(None, "ubuntu:KDE", all), Some("ark"));
        assert_eq!(choose_archiver(Some("ark"), "X-Cinnamon", |p| p != "ark"), Some("engrampa"), "the one set, but not installed");
        assert_eq!(choose_archiver(None, "GNOME", |p| p == "xarchiver"), Some("xarchiver"));
        assert_eq!(choose_archiver(None, "GNOME", |_| false), None);
    }

    #[test]
    fn archive_command_lines() {
        let words = |tool: &str, extract: bool| -> Vec<String> {
            archive_argv(tool, extract, vec!["/a/b.zip".into()]).into_iter().map(|w| w.to_string_lossy().into_owned()).collect()
        };
        assert_eq!(words("ark", true), ["ark", "--batch", "--autodestination", "--autosubfolder", "/a/b.zip"]);
        assert_eq!(words("ark", false), ["ark", "--changetofirstpath", "--add", "--dialog", "/a/b.zip"]);
        assert_eq!(words("engrampa", true), ["engrampa", "--extract-here", "/a/b.zip"]);
        assert_eq!(words("xarchiver", false), ["xarchiver", "--compress", "/a/b.zip"]);
        assert_eq!(words("file-roller", false), ["file-roller", "--add", "/a/b.zip"]);
    }

    #[test]
    fn tab_order_kept_for_tabs_not_shown() {
        let v = |x: &[&str]| x.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        let kept = v(&["basic", "emblems", "permissions", "open-with", "notes", "media"]);
        // A folder (no Open With, no Media Info): Notes dragged to the front.
        // Open With stays after Permissions, Media Info after Notes.
        assert_eq!(merge_tab_order(&kept, &v(&["notes", "basic", "emblems", "permissions"])), v(&["notes", "media", "basic", "emblems", "permissions", "open-with"]));
        // A file, Media Info first: the rest as they were.
        assert_eq!(merge_tab_order(&kept, &v(&["media", "basic", "emblems", "permissions", "open-with", "notes"])), v(&["media", "basic", "emblems", "permissions", "open-with", "notes"]));
        // Every tab shown (Rename All): just the new order.
        assert_eq!(merge_tab_order(&v(&["a", "b", "c"]), &v(&["c", "a", "b"])), v(&["c", "a", "b"]));
    }

    #[test]
    fn bookmark_names_of_shares() {
        assert_eq!(default_bookmark_label("file:///home/u/Backups"), "Backups");
        assert_eq!(default_bookmark_label("smb://server/My%20Share/"), "My Share");
        assert_eq!(default_bookmark_label("smb://server/"), "server");
    }

    #[test]
    fn numbered_groups_in_order() {
        let kf = glib::KeyFile::new();
        kf.load_from_data("[Action 10]\n[Other]\n[Action 2]\n[Action x]\n[Action 0]\n", glib::KeyFileFlags::NONE).unwrap();
        assert_eq!(numbered_groups(&kf, "Action "), ["Action 0", "Action 2", "Action 10"]);
    }

    #[test]
    fn placeholders_expanded() {
        let v = |c| (c == 'a').then(|| std::ffi::OsString::from("A"));
        assert_eq!(expand_placeholders("x %a %% %b 100%", v), "x A % %b 100%");
        assert_eq!(expand_placeholders("%%a%a", v), "%aA");
    }
    use crate::testutil::TestDir;

    fn glob(p: &str, s: &str) -> bool {
        glob_match(&p.chars().collect::<Vec<_>>(), &s.chars().collect::<Vec<_>>())
    }

    #[test]
    fn globs() {
        assert!(glob("*.jpg", "photo.jpg"));
        assert!(!glob("*.jpg", "photo.jpeg"));
        assert!(glob("*", "") && glob("*", "anything"));
        assert!(glob("", "") && !glob("", "x"));
        assert!(glob("IMG_????.*", "IMG_0042.png"));
        assert!(!glob("IMG_????.*", "IMG_42.png"));
        assert!(!glob("?", ""));
        assert!(glob("[ab]*", "beta") && !glob("[ab]*", "gamma"));
        assert!(glob("a*b*c", "aXXbYYc") && !glob("a*b*c", "aXXbYY"));
        assert!(glob("*.tar.*", "x.tar.gz"));
        assert!(glob("ünï*", "ünïcode"));
        // A "[" without a set is an ordinary character.
        assert!(glob("[", "[") && glob("x[", "x[") && glob("[]", "[]"));
    }

    #[test]
    fn glob_with_many_stars_is_quick() {
        let name = "a".repeat(200);
        let t = std::time::Instant::now();
        assert!(!glob("*a*a*a*a*a*a*a*a*b", &name));
        assert!(glob("*a*a*a*a*a*a*a*a*", &name));
        assert!(t.elapsed() < std::time::Duration::from_secs(1), "took {:?}", t.elapsed());
    }

    #[test]
    fn extensions() {
        assert_eq!(split_ext("photo.jpg"), ("photo", ".jpg"));
        assert_eq!(split_ext("a.b.c"), ("a.b", ".c"));
        assert_eq!(split_ext("archive.tar.gz"), ("archive", ".tar.gz"));
        assert_eq!(split_ext("Backup.TAR.XZ"), ("Backup", ".TAR.XZ"));
        assert_eq!(split_ext("Fotos-Ünïcödé.tar.zst"), ("Fotos-Ünïcödé", ".tar.zst"));
        assert_eq!(split_ext("notes"), ("notes", ""));
        // A leading dot alone (hidden files) isn't an extension.
        assert_eq!(split_ext(".bashrc"), (".bashrc", ""));
        assert_eq!(split_ext(".config.bak"), (".config", ".bak"));
        assert_eq!(split_ext(".tar.gz"), (".tar", ".gz"));
        assert_eq!(split_ext(""), ("", ""));
    }

    #[test]
    fn free_names() {
        let t = TestDir::new("names");
        let dir = gio::File::for_path(&t.0);
        assert_eq!(unique_name(&dir, "a.txt", false), "a.txt");
        t.file("a.txt", "");
        assert_eq!(unique_name(&dir, "a.txt", false), "a (2).txt");
        t.file("a (2).txt", "");
        assert_eq!(unique_name(&dir, "a.txt", false), "a (3).txt");
        t.file("b.tar.gz", "");
        assert_eq!(unique_name(&dir, "b.tar.gz", false), "b (2).tar.gz");
        std::fs::create_dir(t.0.join("my.folder")).unwrap();
        // A folder's name has no extension.
        assert_eq!(unique_name(&dir, "my.folder", true), "my.folder (2)");

        assert_eq!(copy_name(&dir, "a.txt", false), "a (copy).txt");
        t.file("a (copy).txt", "");
        assert_eq!(copy_name(&dir, "a.txt", false), "a (copy 2).txt");
        t.file("a (copy 2).txt", "");
        assert_eq!(copy_name(&dir, "a.txt", false), "a (copy 3).txt");
        assert_eq!(copy_name(&dir, "my.folder", true), "my.folder (copy)");
        assert_eq!(copy_name(&dir, ".hidden", false), ".hidden (copy)");
    }

    #[test]
    fn sizes() {
        // One test for both units, as the setting is shared by the whole process.
        assert_eq!(format_size(0), "0 bytes");
        assert_eq!(format_size(1), "1 byte");
        // GLib keeps number and unit together with a no-break space.
        assert_eq!(format_size(1000), "1.0\u{a0}kB");
        assert_eq!(format_size(1_500_000), "1.5\u{a0}MB");
        set_iec_units(true);
        let iec = [format_size(1000), format_size(1024), format_size(1_572_864)];
        set_iec_units(false);
        assert_eq!(iec, ["1000 bytes", "1.0\u{a0}KiB", "1.5\u{a0}MiB"]);
    }

    #[test]
    fn times() {
        assert_eq!(format_time(0), "");
        assert_eq!(format_time(-5), "");
        let now = glib::DateTime::now_local().unwrap();
        assert!(format_time(now.to_unix()).starts_with("Today "));
        assert!(format_time(now.add_days(-1).unwrap().to_unix()).starts_with("Yesterday "));
        // 9 September 2001, 01:46:40 UTC: the 8th or 9th wherever we are.
        assert!(format_time(1_000_000_000).contains("Sep 2001"));
        // The other formats (in this test, as the setting is shared).
        let t = glib::DateTime::from_local(2001, 9, 9, 3, 46, 40.0).unwrap().to_unix();
        set_date_format("iso");
        let iso = format_time(t);
        set_date_format("locale");
        let locale = format_time(t);
        set_date_format("informal");
        assert_eq!(iso, "2001-09-09 03:46:40");
        assert!(locale.contains("2001") && !locale.contains("Today"), "{locale}");
        assert!(format_time(now.to_unix()).starts_with("Today "));
        assert!(format_time_long(1_000_000_000).contains("September 2001"));
    }

    #[test]
    fn local_times_as_glibs() {
        // The cached zone gives what GLib's own local time does, also an
        // hour either side of summer time's start and end (in Europe:
        // 29 March and 25 October 2026, 01:00 UTC).
        let f = "%Y-%m-%d %H:%M:%S %z";
        for t in [0, 1_000_000_000, 1_774_746_000 - 3600, 1_774_746_000, 1_774_746_000 + 3600, 1_792_890_000 - 3600, 1_792_890_000, 1_792_890_000 + 3600] {
            let ours = local_time(t).unwrap().format(f).unwrap();
            let glibs = glib::DateTime::from_unix_local(t).unwrap().format(f).unwrap();
            assert_eq!(ours, glibs, "{t}");
        }
    }

    #[test]
    fn settings_with_bad_lines() {
        let t = crate::testutil::TestDir::new("badconf");
        let p = t.0.join("arkha.conf");
        // One bad line among good ones: the rest is read, the file kept.
        std::fs::write(&p, "[General]\nshow_hidden=true\noops, typed by hand\n[Broken\nsingle_click=true\n").unwrap();
        let kf = load_settings(&p).unwrap();
        assert!(kf.boolean("General", "show_hidden").unwrap());
        assert!(kf.boolean("General", "single_click").unwrap());
        assert!(t.0.join("arkha.conf.broken").exists());
        // A key before any group is left out too.
        std::fs::write(&p, "stray=1\n[General]\nshow_hidden=true\n").unwrap();
        assert!(load_settings(&p).unwrap().boolean("General", "show_hidden").unwrap());
        // A good file: read as it is, nothing kept beside it.
        std::fs::remove_file(t.0.join("arkha.conf.broken")).unwrap();
        std::fs::write(&p, "# note\n[General]\nshow_hidden=false\n").unwrap();
        assert!(!load_settings(&p).unwrap().boolean("General", "show_hidden").unwrap());
        assert!(!t.0.join("arkha.conf.broken").exists());
        assert!(load_settings(&t.0.join("missing.conf")).is_none());
    }

    #[test]
    fn long_names_stay_names() {
        let t = crate::testutil::TestDir::new("longname");
        let dir = gio::File::for_path(&t.0);
        let long = format!("{}.txt", "ä".repeat(124)); // 252 bytes
        std::fs::write(t.0.join(&long), "x").unwrap();
        let copy = copy_name(&dir, &long, false);
        assert!(copy.len() <= 255 && copy.ends_with(" (copy).txt"), "{copy}");
        let other = unique_name(&dir, &long, false);
        assert!(other.len() <= 255 && other.ends_with(" (2).txt"), "{other}");
        // Usable: made, read back.
        std::fs::write(t.0.join(&copy), "y").unwrap();
        assert_eq!(std::fs::read_to_string(t.0.join(&copy)).unwrap(), "y");
        // Short names as they were.
        assert_eq!(copy_name(&dir, "a.txt", false), "a (copy).txt");
    }

    #[test]
    fn direction_marks_shown() {
        assert_eq!(visible_controls("photo\u{202e}gpj.exe"), "photo<U+202E>gpj.exe");
        assert_eq!(visible_controls("a\u{2066}b\u{2069}"), "a<U+2066>b<U+2069>");
        // Other text as it is, right-to-left scripts too.
        assert_eq!(visible_controls("שלום.txt"), "שלום.txt");
    }

    #[test]
    fn terminals() {
        let t = |term: &str, p: &str| terminal_run_argv(term, std::ffi::OsStr::new(p)).iter().map(|a| a.to_string_lossy().to_string()).collect::<Vec<_>>();
        assert_eq!(t("gnome-terminal", "/a b/run"), ["gnome-terminal", "--", "/a b/run"]);
        assert_eq!(t("/usr/bin/xfce4-terminal", "x"), ["/usr/bin/xfce4-terminal", "-x", "x"]);
        assert_eq!(t("mate-terminal", "x"), ["mate-terminal", "-x", "x"]);
        assert_eq!(t("kitty", "x"), ["kitty", "x"]);
        assert_eq!(t("xterm", "x"), ["xterm", "-e", "x"]);
    }

    #[test]
    fn locations_and_hidden_names() {
        assert_eq!(pretty_location(&home_file()), "~");
        assert_eq!(pretty_location(&gio::File::for_path("/a b/c")), "/a b/c");
        assert_eq!(pretty_location(&gio::File::for_uri("sftp://host/My%20Files")), "sftp://host/My Files");
        assert!(name_hidden(".bashrc") && name_hidden("notes.txt~"));
        assert!(!name_hidden("a.txt") && !name_hidden("a~b"));
    }

    #[test]
    fn info_fallbacks() {
        let i = gio::FileInfo::new();
        assert!(!info_is_hidden(&i) && !info_is_backup(&i) && !info_is_symlink(&i));
        assert_eq!(info_content_type(&i), None);
        // Without the attributes (MTP), the name decides.
        i.set_name(".notes.txt~");
        assert!(info_is_hidden(&i) && info_is_backup(&i));
        i.set_name("photo.png");
        assert!(!info_is_hidden(&i) && !info_is_backup(&i));
        assert_eq!(info_content_type(&i).as_deref(), Some("image/png"));
        // What the backend reports wins.
        i.set_name(".x~");
        i.set_is_hidden(false);
        i.set_attribute_boolean("standard::is-backup", false);
        assert!(!info_is_hidden(&i) && !info_is_backup(&i));
        i.set_content_type("text/plain");
        assert_eq!(info_content_type(&i).as_deref(), Some("text/plain"));
        i.set_is_symlink(true);
        assert!(info_is_symlink(&i));
    }

    #[test]
    fn bookmarks() {
        let b = parse_bookmarks("file:///home/u/Music\nfile:///home/u/My%20Docs Work stuff\n\nsftp://host/srv Server\n");
        let want: Vec<(String, String)> = [
            ("file:///home/u/Music", "Music"),
            ("file:///home/u/My%20Docs", "Work stuff"),
            ("sftp://host/srv", "Server"),
        ]
        .map(|(u, l)| (u.to_string(), l.to_string()))
        .to_vec();
        assert_eq!(b, want);
        assert_eq!(parse_bookmarks("file:///a%20b")[0].1, "a b");

        let t = TestDir::new("bookmarks");
        let p = t.0.join("gtk-3.0").join("bookmarks");
        assert!(add_bookmark_to(&p, "file:///a").unwrap());
        assert!(!add_bookmark_to(&p, "file:///a").unwrap());
        // A labelled line without a final newline.
        std::fs::write(&p, "file:///a A").unwrap();
        assert!(!add_bookmark_to(&p, "file:///a").unwrap());
        assert!(add_bookmark_to(&p, "file:///ab").unwrap());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "file:///a A\nfile:///ab\n");
        // A label that's just the folder's name isn't written; an empty one
        // means the folder's name.
        edit_bookmarks_at(&p, |b| {
            b[0].1 = "a".into();
            b[1].1 = "Other".into();
            b.push(("file:///c".into(), String::new()));
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "file:///a\nfile:///ab Other\nfile:///c\n");
        // Nothing changed: the file isn't written.
        std::fs::write(&p, "file:///a\n\nfile:///b B").unwrap();
        edit_bookmarks_at(&p, |_| {}).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "file:///a\n\nfile:///b B");
    }

    #[test]
    fn bookmarks_with_names() {
        let t = TestDir::new("bookmarks-named");
        let p = t.0.join("gtk-3.0").join("bookmarks");
        // New ones at the end, with their names (or the folder's name).
        bookmark_named_at(&p, "sftp://debian/root", "Debian VM").unwrap();
        bookmark_named_at(&p, "smb://nas/Public", "").unwrap();
        bookmark_named_at(&p, "file:///srv", " Server ").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "sftp://debian/root Debian VM\nsmb://nas/Public\nfile:///srv Server\n");
        // An existing one keeps its place and gets the new name; an empty
        // name leaves its label alone.
        bookmark_named_at(&p, "smb://nas/Public", "NAS").unwrap();
        bookmark_named_at(&p, "sftp://debian/root", "").unwrap();
        bookmark_named_at(&p, "file:///srv", "Data").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "sftp://debian/root Debian VM\nsmb://nas/Public NAS\nfile:///srv Data\n");
        assert_eq!(bookmark_lines(&std::fs::read_to_string(&p).unwrap()).len(), 3);
    }

    #[test]
    fn random_names_patterns_and_bookmarks() {
        use crate::testutil::cases;
        cases(3000, |r| {
            let name = r.text(20);
            let (a, b) = split_ext(&name);
            assert_eq!(format!("{a}{b}"), name, "split_ext({name:?})");
            let n: Vec<char> = name.chars().collect();
            // Without its special characters a pattern matches itself; "*" anything.
            let plain: Vec<char> = n.iter().copied().filter(|c| !"*?[".contains(*c)).collect();
            assert!(glob_match(&plain, &plain), "{plain:?}");
            assert!(glob_match(&['*'], &n));
            let pat: Vec<char> = r.text(12).chars().collect();
            let _ = glob_match(&pat, &n);
            let _ = name_hidden(&name);
        });
        // Bookmarks read back as written.
        let t = TestDir::new("random-bookmarks");
        let p = t.0.join("bookmarks");
        let uri_chars: Vec<char> = "abcäß日/%:.-_~".chars().collect();
        cases(300, |r| {
            let list: Vec<(String, String)> = (0..r.below(6))
                .map(|_| (format!("file:///{}", r.text_of(&uri_chars, 12)), r.text(10).replace(['\n', '\r'], "")))
                .collect();
            let _ = std::fs::remove_file(&p);
            edit_bookmarks_at(&p, |b| *b = list.clone()).unwrap();
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            let want: Vec<(String, String)> = list
                .iter()
                .map(|(u, l)| (u.clone(), if *l == default_bookmark_label(u) { String::new() } else { l.clone() }))
                .collect();
            assert_eq!(bookmark_lines(&text), want, "{text:?}");
        });
    }
}
