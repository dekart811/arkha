//! File operations (copy, move, link, trash, delete) running on a worker
//! thread, with a progress window and conflict/error dialogs on the UI side.

use gio::prelude::*;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crate::i18n::{fill, n_, tr, trn};
use crate::thumbcache;
use crate::util;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Copy,
    Move,
    Link,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub src: String,
    pub dest_dir: String,
    /// Explicit target name (used by restore-from-trash). None = keep source name.
    pub name: Option<String>,
}

/// One thing a copy, move or link did: `src` went to `dst`.
/// `replaced`: something that was at `dst` before was replaced.
#[derive(Clone, Debug)]
struct Moved {
    src: String,
    dst: String,
    replaced: bool,
}

#[derive(Clone, Debug)]
pub enum Job {
    /// `check`: each copied file read back and compared with its original
    /// (CRC32); a move deletes an original only once its copy matched.
    Transfer { kind: Kind, items: Vec<Item>, check: bool },
    Trash(Vec<String>),
    Delete(Vec<String>),
    EmptyTrash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    Skip,
    Replace, // "Merge" for folders
    KeepBoth,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrChoice {
    Skip,
    SkipAll,
    Retry,
    Cancel,
    DeletePermanently,
}

pub struct ConflictReq {
    pub name: String,
    pub dest_dir: String,
    pub src_is_dir: bool,
    pub dest_is_dir: bool,
    pub src_desc: String,
    pub dest_desc: String,
}

enum Msg {
    Progress { detail: String, fraction: f64 },
    /// Answered with the resolution, whether it applies to all conflicts,
    /// and for Keep Both a name typed for the new item.
    Conflict(ConflictReq, mpsc::Sender<(Resolution, bool, Option<String>)>),
    Error {
        title: String,
        detail: String,
        offer_delete: bool,
        reply: mpsc::Sender<ErrChoice>,
    },
    Done(Vec<String>, Vec<Moved>),
    /// A checked copy's outcome: how many files matched, the names of
    /// those that didn't.
    Checked(u64, Vec<String>),
}

struct Cancelled;
type Flow = Result<(), Cancelled>;

struct Worker {
    tx: async_channel::Sender<Msg>,
    cancel: gio::Cancellable,
    /// What was copied, moved or linked, for Edit → Undo.
    undo: Vec<Moved>,
    /// Where thumbnails are kept (see thumbcache).
    thumbs: std::path::PathBuf,
    /// The destination keeps names as Windows does (FAT, exFAT, NTFS):
    /// see windows_name.
    windows_names: bool,
    /// The destination is FAT32: no file of 4 GB or more.
    fat32: bool,
    /// Originals whose copies got their thumbnails in a move between
    /// drives, theirs going once they're deleted (thumbnails_of_deleted).
    thumbs_of_copied: Vec<String>,
    total_bytes: u64,
    done_bytes: u64,
    total_items: u64,
    done_items: u64,
    by_bytes: bool,
    file_all: Option<Resolution>,
    dir_all: Option<Resolution>,
    skip_errors: bool,
    skipped: u64,
    /// The name typed in the conflict dialog for Keep Both (see keep_both).
    new_name: Option<String>,
    last: Instant,
    /// A transfer's kind, for its progress texts (see doing).
    kind: Kind,
    /// Set from the progress window's pause button.
    paused: Arc<AtomicBool>,
    clock: Clock,
    moves: MoveCount,
    /// Copies are read back and compared (see Job::Transfer).
    check: bool,
    checked: u64,
    check_failed: Vec<String>,
}

/// A move's items, for counting it in bytes once any of it has to be
/// copied (to another file system, or between datasets behind a network
/// share): one total for the whole move, not one for each item.
#[derive(Default)]
struct MoveCount {
    srcs: Vec<gio::File>,
    /// The item being moved.
    at: usize,
    /// The items' sizes, counted from the first one that had to be copied
    /// (those before it are 0).
    sizes: Option<Vec<u64>>,
}

/// How long a job has been at work, as Caja's job timer: time paused or
/// waiting for an answer in a dialog is left out (the rate, and the time
/// left from it, would look worse than they are).
#[derive(Clone)]
struct Clock {
    start: Instant,
    /// Milliseconds paused or waiting.
    idle: Arc<std::sync::atomic::AtomicU64>,
}

impl Clock {
    fn new() -> Clock {
        Clock { start: Instant::now(), idle: Arc::default() }
    }

    fn working(&self) -> Duration {
        self.start.elapsed().saturating_sub(Duration::from_millis(self.idle.load(Ordering::Relaxed)))
    }

    /// Runs `f` (a wait) with its time left out.
    fn idle<R>(&self, f: impl FnOnce() -> R) -> R {
        let t = Instant::now();
        let r = f();
        self.idle.fetch_add(t.elapsed().as_millis() as u64, Ordering::Relaxed);
        r
    }
}

/// Blocks while the job is paused (until resumed or cancelled).
fn wait_while_paused(paused: &AtomicBool, cancel: &gio::Cancellable, clock: &Clock) {
    if paused.load(Ordering::Relaxed) {
        clock.idle(|| {
            while paused.load(Ordering::Relaxed) && !cancel.is_cancelled() {
                std::thread::sleep(Duration::from_millis(100));
            }
        });
    }
}

/// The rate is guessed from the job's average so far: before this it's
/// too unsteady to tell the time left, as in Caja.
const RELIABLE_RATE_AFTER: Duration = Duration::from_secs(15);

/// A time left as Caja words it: "40 seconds", "3 minutes", "1 hour, 20
/// minutes", "approximately 5 hours".
fn format_time_left(secs: u64) -> String {
    let seconds = |n: u64| fill(&trn("{n} second", "{n} seconds", n), &[("n", &n)]);
    let minutes = |n: u64| fill(&trn("{n} minute", "{n} minutes", n), &[("n", &n)]);
    let hours = |n: u64| fill(&trn("{n} hour", "{n} hours", n), &[("n", &n)]);
    match secs {
        0..60 => seconds(secs),
        60..3600 => minutes(secs / 60),
        // TRANSLATORS: "2 hours, 5 minutes".
        3600..14400 => fill(&tr("{hours}, {minutes}"), &[("hours", &hours(secs / 3600)), ("minutes", &minutes(secs % 3600 / 60))]),
        _ => fill(&tr("approximately {hours}"), &[("hours", &hours(secs / 3600))]),
    }
}

/// "Copying “a.txt”", "Moving …", "Linking …" for a transfer's progress.
fn doing(kind: Kind, name: &str) -> String {
    // check-placeholders: filled later
    let template = match kind {
        Kind::Copy => tr("Copying “{name}”"),
        Kind::Move => tr("Moving “{name}”"),
        Kind::Link => tr("Linking “{name}”"),
    };
    fill(&template, &[("name", &name)])
}

/// Caja's progress details for a job counted in bytes: "4 MB of 10 MB",
/// then, once the rate can be told, "4 MB of 10 MB — 2 minutes left
/// (1 MB/sec)".
pub fn bytes_detail(done: u64, total: u64, working: Duration) -> String {
    let total = total.max(done);
    let mut s = fill(&tr("{done} of {total}"), &[("done", &util::format_size(done)), ("total", &util::format_size(total))]);
    if working >= RELIABLE_RATE_AFTER && done > 0 {
        let rate = done as f64 / working.as_secs_f64();
        let left = ((total - done) as f64 / rate) as u64;
        // TRANSLATORS: after "4 MB of 10 MB": " — 2 minutes left (1 MB/sec)".
        s.push_str(&fill(&tr(" — {time} left ({rate}/sec)"), &[("time", &format_time_left(left)), ("rate", &util::format_size(rate as u64))]));
    }
    s
}

/// The same for a job counted in items: "40 of 120 items — 1 minute left".
fn items_detail(done: u64, total: u64, working: Duration) -> String {
    let total = total.max(done);
    let mut s = fill(&trn("{done} of {total} item", "{done} of {total} items", total), &[("done", &done), ("total", &total)]);
    if working >= RELIABLE_RATE_AFTER && done > 0 {
        let rate = done as f64 / working.as_secs_f64();
        s.push_str(&fill(&tr(" — {time} left"), &[("time", &format_time_left(((total - done) as f64 / rate) as u64))]));
    }
    s
}

const NOFOLLOW: gio::FileQueryInfoFlags = gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS;

fn kind_of(f: &gio::File) -> Option<gio::FileType> {
    f.query_info("standard::type", NOFOLLOW, gio::Cancellable::NONE)
        .ok()
        .map(|i| i.file_type())
}

fn describe(f: &gio::File) -> String {
    match f.query_info(
        "standard::size,standard::type,time::modified",
        NOFOLLOW,
        gio::Cancellable::NONE,
    ) {
        Ok(i) => {
            let when = util::format_time(i.attribute_uint64("time::modified") as i64);
            if i.file_type() == gio::FileType::Directory {
                fill(&tr("Folder, modified {date}"), &[("date", &when)])
            } else {
                fill(&tr("{size}, modified {date}"), &[("size", &util::format_size(i.size() as u64)), ("date", &when)])
            }
        }
        Err(_) => String::new(),
    }
}

fn base_name(f: &gio::File) -> String {
    f.basename()
        .map(|b| b.to_string_lossy().to_string())
        .unwrap_or_default()
}

impl Worker {
    fn new(tx: async_channel::Sender<Msg>, cancel: gio::Cancellable, paused: Arc<AtomicBool>) -> Worker {
        Worker {
            tx,
            cancel,
            undo: vec![],
            thumbs: thumbcache::root(),
            windows_names: false,
            fat32: false,
            thumbs_of_copied: vec![],
            total_bytes: 0,
            done_bytes: 0,
            total_items: 0,
            done_items: 0,
            by_bytes: false,
            file_all: None,
            dir_all: None,
            skip_errors: false,
            skipped: 0,
            new_name: None,
            // So the first progress is shown right away.
            last: Instant::now() - Duration::from_secs(1),
            // Only transfers read it (each sets its own).
            kind: Kind::Copy,
            paused,
            clock: Clock::new(),
            moves: MoveCount::default(),
            check: false,
            checked: 0,
            check_failed: vec![],
        }
    }

    /// How many times each byte copied is counted: also when it's checked.
    fn byte_passes(&self) -> u64 {
        if self.check {
            2
        } else {
            1
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    fn progress(&mut self, detail: &str, force: bool) {
        // Every step of a job reports progress, so this is where it pauses.
        wait_while_paused(&self.paused, &self.cancel, &self.clock);
        if !force && self.last.elapsed() < Duration::from_millis(80) {
            return;
        }
        self.last = Instant::now();
        let fraction = if self.by_bytes && self.total_bytes > 0 {
            self.done_bytes as f64 / self.total_bytes as f64
        } else if self.total_items > 0 {
            self.done_items as f64 / self.total_items as f64
        } else {
            0.0
        };
        let working = self.clock.working();
        let detail = if self.by_bytes {
            format!("{detail}\n{}", bytes_detail(self.done_bytes, self.total_bytes, working))
        } else {
            format!("{detail}\n{}", items_detail(self.done_items, self.total_items, working))
        };
        let _ = self.tx.send_blocking(Msg::Progress {
            detail,
            fraction: fraction.clamp(0.0, 1.0),
        });
    }

    fn ask_error(&mut self, title: String, err: &glib::Error, offer_delete: bool) -> ErrChoice {
        if err.matches(gio::IOErrorEnum::Cancelled) {
            return ErrChoice::Cancel;
        }
        if self.skip_errors {
            self.skipped += 1;
            return ErrChoice::Skip;
        }
        let (rtx, rrx) = mpsc::channel();
        let _ = self.tx.send_blocking(Msg::Error {
            title,
            detail: err.message().to_string(),
            offer_delete,
            reply: rtx,
        });
        let c = self.clock.idle(|| rrx.recv().unwrap_or(ErrChoice::Cancel));
        match c {
            ErrChoice::SkipAll => {
                self.skip_errors = true;
                self.skipped += 1;
                ErrChoice::Skip
            }
            ErrChoice::Skip => {
                self.skipped += 1;
                c
            }
            ErrChoice::Cancel => {
                self.cancel.cancel();
                c
            }
            _ => c,
        }
    }

    fn ask_conflict(&mut self, src: &gio::File, dst: &gio::File, src_dir: bool, dst_dir: bool) -> Resolution {
        let both_dirs = src_dir && dst_dir;
        let remembered = if both_dirs { self.dir_all } else { self.file_all };
        if let Some(r) = remembered {
            return r;
        }
        let (rtx, rrx) = mpsc::channel();
        let req = ConflictReq {
            name: base_name(dst),
            dest_dir: dst
                .parent()
                .map(|p| util::display_name(&p))
                .unwrap_or_default(),
            src_is_dir: src_dir,
            dest_is_dir: dst_dir,
            src_desc: describe(src),
            dest_desc: describe(dst),
        };
        let _ = self.tx.send_blocking(Msg::Conflict(req, rtx));
        let (r, all, name) = self.clock.idle(|| rrx.recv().unwrap_or((Resolution::Cancel, false, None)));
        self.new_name = name.filter(|_| r == Resolution::KeepBoth);
        if all {
            if both_dirs {
                self.dir_all = Some(r);
            } else {
                self.file_all = Some(r);
            }
        }
        if r == Resolution::Cancel {
            self.cancel.cancel();
        }
        r
    }

    /// Where Keep Both puts the item that would have gone to `dst`: under
    /// the name typed in the conflict dialog, else a free one ("name (2)").
    fn keep_both(&mut self, dst: &gio::File, is_dir: bool) -> gio::File {
        let parent = dst.parent().unwrap();
        match self.new_name.take() {
            Some(name) => parent.child(name),
            None => parent.child(util::unique_name(&parent, &base_name(dst), is_dir)),
        }
    }

    /// Entries of folder `f`. Read errors go through the error dialog: Retry reads
    /// the folder again, Skip gives None and counts as skipped, so a move that
    /// couldn't copy everything keeps its source.
    /// `failed`: the error's title, with {name} (marked with n_()).
    fn list_children(&mut self, f: &gio::File, failed: &str) -> Result<Option<Vec<std::path::PathBuf>>, Cancelled> {
        loop {
            if self.cancelled() {
                return Err(Cancelled);
            }
            let err = match f.enumerate_children("standard::name", NOFOLLOW, Some(&self.cancel)) {
                Ok(en) => {
                    let mut v = vec![];
                    loop {
                        match en.next_file(Some(&self.cancel)) {
                            Ok(Some(ci)) => v.push(ci.name()),
                            Ok(None) => return Ok(Some(v)),
                            Err(e) => break e,
                        }
                    }
                }
                Err(e) => e,
            };
            match self.ask_error(fill(&tr(failed), &[("name", &base_name(f))]), &err, false) {
                ErrChoice::Retry => continue,
                ErrChoice::Cancel => return Err(Cancelled),
                _ => return Ok(None),
            }
        }
    }

    /// Count bytes and items below `f` (not following symlinks).
    /// What a file copy is called: part of a move is copied too.
    fn copy_kind(&self) -> Kind {
        if self.kind == Kind::Move {
            Kind::Move
        } else {
            Kind::Copy
        }
    }

    /// From the first item of a move that has to be copied on: count the
    /// bytes of it and the items after it, and show the move's progress in
    /// them from now on.
    fn count_move_bytes(&mut self) {
        if self.moves.sizes.is_some() {
            return;
        }
        let at = self.moves.at.min(self.moves.srcs.len());
        let mut sizes = vec![0; at];
        for f in &self.moves.srcs[at..] {
            let (mut b, mut n) = (0, 0);
            self.scan(f, &mut b, &mut n);
            sizes.push(b * self.byte_passes());
        }
        self.total_bytes = sizes.iter().sum();
        self.done_bytes = 0;
        self.by_bytes = true;
        self.moves.sizes = Some(sizes);
        let name = self.moves.srcs.get(at).map(base_name).unwrap_or_default();
        self.progress(&doing(self.kind, &name), true);
    }

    fn scan(&self, f: &gio::File, bytes: &mut u64, items: &mut u64) {
        if self.cancelled() {
            return;
        }
        let Ok(info) = f.query_info("standard::type,standard::size", NOFOLLOW, Some(&self.cancel)) else {
            *items += 1;
            return;
        };
        *items += 1;
        if info.file_type() == gio::FileType::Directory {
            if let Ok(en) = f.enumerate_children("standard::name,standard::type,standard::size", NOFOLLOW, Some(&self.cancel)) {
                while let Ok(Some(ci)) = en.next_file(Some(&self.cancel)) {
                    let child = f.child(ci.name());
                    if ci.file_type() == gio::FileType::Directory {
                        self.scan(&child, bytes, items);
                    } else {
                        *items += 1;
                        *bytes += ci.size().max(0) as u64;
                    }
                }
            }
        } else {
            *bytes += info.size().max(0) as u64;
        }
    }

    /// Deletes `f` and all inside it. `inner`: it's inside what was asked
    /// to be deleted (see remove_path).
    /// Deletes `f`, a folder with everything in it. `thumb`: the URI its
    /// thumbnails are kept under (a trashed item's: where it came from),
    /// removed with it; None leaves them (an original moved: they go with
    /// the move).
    fn delete_rec(&mut self, f: &gio::File, inner: bool, thumb: Option<String>) -> Flow {
        let remove = |f: &gio::File, cancel: &gio::Cancellable| match f.path().filter(|_| inner) {
            Some(path) => remove_path(&path),
            None => f.delete(Some(cancel)),
        };
        loop {
            if self.cancelled() {
                return Err(Cancelled);
            }
            match remove(f, &self.cancel) {
                Ok(()) => {
                    self.done_items += 1;
                    if let Some(t) = &thumb {
                        thumbcache::forget(&self.thumbs, t);
                    }
                    self.progress(&fill(&tr("Deleting “{name}”"), &[("name", &base_name(f))]), false);
                    return Ok(());
                }
                Err(e) if e.matches(gio::IOErrorEnum::NotEmpty) => {
                    // check-placeholders: filled later
                    let Some(children) = self.list_children(f, n_("Error while deleting “{name}”"))? else {
                        return Ok(());
                    };
                    let before = self.skipped;
                    for c in children {
                        let t = thumb.as_ref().map(|t| gio::File::for_uri(t).child(&c).uri().to_string());
                        self.delete_rec(&f.child(c), true, t)?;
                    }
                    if self.skipped != before {
                        return Ok(()); // something inside was kept
                    }
                    // try deleting the (now empty) dir again
                    match remove(f, &self.cancel) {
                        Ok(()) => {
                            self.done_items += 1;
                            if let Some(t) = &thumb {
                                thumbcache::forget(&self.thumbs, t);
                            }
                            return Ok(());
                        }
                        Err(e) => {
                            return match self.ask_error(fill(&tr("Error while deleting “{name}”"), &[("name", &base_name(f))]), &e, false) {
                                ErrChoice::Retry => continue,
                                ErrChoice::Cancel => Err(Cancelled),
                                _ => Ok(()),
                            }
                        }
                    }
                }
                Err(e) if e.matches(gio::IOErrorEnum::NotFound) => return Ok(()),
                Err(e) => {
                    match self.ask_error(fill(&tr("Error while deleting “{name}”"), &[("name", &base_name(f))]), &e, false) {
                        ErrChoice::Retry => continue,
                        ErrChoice::Cancel => return Err(Cancelled),
                        _ => return Ok(()),
                    }
                }
            }
        }
    }

    /// Notes for Edit → Undo that `src` went to `dst`.
    /// In a move, `src`'s thumbnails go with it to `dst` (a folder's: those
    /// of all in it), as soon as it's there: before the folder shown asks
    /// for them, which would make them again. A folder's on a thread of
    /// their own: going through a big one took the move a while.
    fn thumbnails_moved(&self, src: &gio::File, dst: &gio::File) {
        if self.kind != Kind::Move {
            return;
        }
        let (root, items) = (self.thumbs.clone(), vec![(src.uri().to_string(), dst.uri().to_string())]);
        if kind_of(dst) == Some(gio::FileType::Directory) {
            std::thread::spawn(move || thumbcache::moved(&root, &items));
        } else {
            thumbcache::moved(&root, &items);
        }
    }

    /// In a move between drives (copied file by file, the originals deleted
    /// at the end): the copy of `src` gets its thumbnails at once, the
    /// original keeps them until it's deleted (thumbnails_of_deleted).
    fn thumbnails_copied(&mut self, src: &gio::File, dst: &gio::File) {
        if self.kind == Kind::Move {
            let src = src.uri().to_string();
            thumbcache::copied(&self.thumbs, &[(src.clone(), dst.uri().to_string())]);
            self.thumbs_of_copied.push(src);
        }
    }

    /// The originals' thumbnails, once a move between drives deleted them
    /// (those still there, kept by a skip, keep theirs).
    fn thumbnails_of_deleted(&mut self) {
        for uri in std::mem::take(&mut self.thumbs_of_copied) {
            if !gio::File::for_uri(&uri).query_exists(gio::Cancellable::NONE) {
                thumbcache::forget(&self.thumbs, &uri);
            }
        }
    }

    fn record(&mut self, src: &gio::File, dst: &gio::File, replaced: bool) {
        self.undo.push(Moved { src: src.uri().to_string(), dst: dst.uri().to_string(), replaced });
    }

    /// `overwrite`: the user already chose Replace for `dst` (a move falling
    /// back to copy across filesystems), so don't ask again. `record`: note
    /// `dst` for Undo if it's made here (false inside a folder made here:
    /// undoing takes back the folder with all in it).
    fn copy_rec(&mut self, src: &gio::File, dst: &gio::File, overwrite: bool, created: &mut Option<String>, record: bool) -> Flow {
        if self.cancelled() {
            return Err(Cancelled);
        }
        let info = loop {
            match src.query_info("standard::type,standard::size,unix::mode", NOFOLLOW, Some(&self.cancel)) {
                Ok(i) => break i,
                Err(e) => match self.ask_error(fill(&tr("Error while copying “{name}”"), &[("name", &base_name(src))]), &e, false) {
                    ErrChoice::Retry => continue,
                    ErrChoice::Cancel => return Err(Cancelled),
                    _ => return Ok(()),
                },
            }
        };
        let mut dst = dst.clone();
        // Something at `dst` was replaced; or, for folders, merged into.
        let mut replaced = false;
        let mut merged = false;
        if info.file_type() == gio::FileType::Directory {
            // Create (or merge into) the destination folder.
            loop {
                match dst.make_directory(Some(&self.cancel)) {
                    Ok(()) => break,
                    Err(e) if e.matches(gio::IOErrorEnum::Exists) => {
                        let dst_dir = kind_of(&dst) == Some(gio::FileType::Directory);
                        let r = if overwrite { Resolution::Replace } else { self.ask_conflict(src, &dst, true, dst_dir) };
                        match r {
                            Resolution::Skip => {
                                self.skipped += 1;
                                let (mut b, mut n) = (0, 0);
                                self.scan(src, &mut b, &mut n);
                                self.done_bytes += b;
                                self.done_items += n;
                                return Ok(());
                            }
                            Resolution::Replace => {
                                if dst_dir {
                                    merged = true;
                                    break;
                                }
                                let before = self.skipped;
                                self.delete_rec(&dst, false, Some(dst.uri().to_string()))?;
                                if self.skipped != before {
                                    return Ok(()); // couldn't remove it, give up on this item
                                }
                                replaced = true;
                            }
                            Resolution::KeepBoth => dst = self.keep_both(&dst, true),
                            Resolution::Cancel => return Err(Cancelled),
                        }
                    }
                    Err(e) => match self.ask_error(fill(&tr("Error while copying “{name}”"), &[("name", &base_name(src))]), &e, false) {
                        ErrChoice::Retry => continue,
                        ErrChoice::Cancel => return Err(Cancelled),
                        _ => return Ok(()),
                    },
                }
            }
            if created.is_none() {
                *created = Some(dst.uri().to_string());
            }
            // A merged folder isn't the copy's own: what goes into it is.
            if record && !merged {
                self.record(src, &dst, replaced);
            }
            self.done_items += 1;
            // check-placeholders: filled later
            let Some(children) = self.list_children(src, n_("Error while reading “{name}”"))? else {
                return Ok(());
            };
            let mut none = None;
            for c in children {
                let to = if self.windows_names { dst.child(windows_name(&c.to_string_lossy())) } else { dst.child(&c) };
                self.copy_rec(&src.child(&c), &to, false, &mut none, record && merged)?;
            }
            // Re-read the mode: it may have changed since `info` (e.g. the user
            // fixed an unreadable folder and pressed Retry).
            let mode = src
                .query_info("unix::mode", NOFOLLOW, gio::Cancellable::NONE)
                .ok()
                .filter(|i| i.has_attribute("unix::mode"))
                .map(|i| i.attribute_uint32("unix::mode"))
                .or_else(|| info.has_attribute("unix::mode").then(|| info.attribute_uint32("unix::mode")));
            if let Some(mode) = mode {
                let _ = dst.set_attribute_uint32("unix::mode", mode, NOFOLLOW, gio::Cancellable::NONE);
            }
            return Ok(());
        }

        // Regular file / symlink / special.
        let size = info.size().max(0) as u64;
        let mut flags = gio::FileCopyFlags::NOFOLLOW_SYMLINKS | gio::FileCopyFlags::ALL_METADATA;
        let name = base_name(src);
        // Said at once, not after writing 4 GB of it.
        if self.fat32 && size > u64::from(u32::MAX) {
            let e = glib::Error::new(gio::IOErrorEnum::FilenameTooLong, &tr("The drive is formatted as FAT32, which holds no file of 4 GB or more. Formatted as exFAT it could hold it."));
            return match self.ask_error(fill(&tr("“{name}” is too big for the drive"), &[("name", &name)]), &e, false) {
                ErrChoice::Cancel => Err(Cancelled),
                _ => Ok(()),
            };
        }
        loop {
            let base = self.done_bytes;
            let total = self.total_bytes;
            let tx = self.tx.clone();
            let mut last = Instant::now();
            let label = doing(self.copy_kind(), &name);
            let (paused, cancel, clock) = (self.paused.clone(), self.cancel.clone(), self.clock.clone());
            let mut cb = |cur: i64, _tot: i64| {
                // Pausing in the middle of a big file stops the copy here.
                wait_while_paused(&paused, &cancel, &clock);
                if last.elapsed() > Duration::from_millis(100) && total > 0 {
                    last = Instant::now();
                    let done = base + cur.max(0) as u64;
                    let _ = tx.send_blocking(Msg::Progress {
                        detail: format!("{label}\n{}", bytes_detail(done, total, clock.working())),
                        fraction: (done as f64 / total as f64).clamp(0.0, 1.0),
                    });
                }
            };
            // Regular files between local paths (local disks, and network
            // shares mounted by the kernel) are copied here, in pieces.
            let pieces = if info.file_type() == gio::FileType::Regular && src.is_native() && dst.is_native() {
                match (src.path(), dst.path()) {
                    (Some(s), Some(d)) => copy_in_pieces(&s, &d, flags.contains(gio::FileCopyFlags::OVERWRITE), &self.cancel, &mut cb),
                    _ => None,
                }
            } else {
                None
            };
            let res = match pieces.map_or_else(|| src.copy(&dst, flags, Some(&self.cancel), Some(&mut cb)), |r| {
                // GIO copies these along with the data.
                if r.is_ok() {
                    let _ = src.copy_attributes(&dst, gio::FileCopyFlags::ALL_METADATA | gio::FileCopyFlags::NOFOLLOW_SYMLINKS, Some(&self.cancel));
                }
                r
            }) {
                // Phones (MTP) often can't copy a file to another place on
                // themselves ("could not copy object"), and GIO doesn't fall
                // back for that error: copy the data through here instead.
                Err(e) if same_mtp_device(src, &dst) && !e.matches(gio::IOErrorEnum::Exists) && !e.matches(gio::IOErrorEnum::Cancelled) => {
                    stream_copy(src, &dst, flags.contains(gio::FileCopyFlags::OVERWRITE), &self.cancel, &mut cb)
                }
                r => r,
            };
            if let Err(e) = &res {
                // GIO leaves what it wrote of a cancelled or failed copy
                // (and a retry would then find it in the way). Only a file
                // made here: one being replaced may still be the old one.
                if !e.matches(gio::IOErrorEnum::Exists) && !flags.contains(gio::FileCopyFlags::OVERWRITE) {
                    let _ = dst.delete(gio::Cancellable::NONE);
                }
            }
            match res {
                Ok(()) => {
                    // libmtp creates every file it downloads as rwxr-----:
                    // give files from a phone the usual permissions instead.
                    if is_mtp(src) && dst.path().is_some() {
                        let _ = dst.set_attribute_uint32("unix::mode", 0o666 & !umask(), gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS, Some(&self.cancel));
                    }
                    if self.check && info.file_type() == gio::FileType::Regular {
                        let failure = match self.verify(src, &dst, size) {
                            Ok(true) => None,
                            Ok(false) => Some((
                                fill(&tr("“{name}” is different after copying"), &[("name", &name)]),
                                glib::Error::new(gio::IOErrorEnum::Failed, &tr("The copy doesn’t match the original (their CRC32 checksums differ): it was changed on the way. Retry copies it again; Skip keeps it as it is (and a moved file’s original too).")),
                            )),
                            Err(e) => Some((fill(&tr("Error while checking “{name}”"), &[("name", &name)]), e)),
                        };
                        if let Some((title, e)) = failure {
                            match self.ask_error(title, &e, false) {
                                ErrChoice::Retry => {
                                    flags |= gio::FileCopyFlags::OVERWRITE;
                                    continue;
                                }
                                ErrChoice::Cancel => return Err(Cancelled),
                                _ => {
                                    // Skipped: left as it is, not noted for
                                    // Undo, and a move keeps its original.
                                    self.check_failed.push(util::display_name(&dst));
                                    self.done_bytes += size * self.byte_passes();
                                    self.done_items += 1;
                                    return Ok(());
                                }
                            }
                        }
                        self.checked += 1;
                        self.done_bytes += size;
                    }
                    self.done_bytes += size;
                    self.done_items += 1;
                    if created.is_none() {
                        *created = Some(dst.uri().to_string());
                    }
                    if record {
                        self.record(src, &dst, replaced || flags.contains(gio::FileCopyFlags::OVERWRITE));
                    }
                    // A move across drives copies file by file.
                    self.thumbnails_copied(src, &dst);
                    self.progress(&doing(self.copy_kind(), &name), false);
                    return Ok(());
                }
                Err(e) if e.matches(gio::IOErrorEnum::Exists) => {
                    let dst_dir = kind_of(&dst) == Some(gio::FileType::Directory);
                    let r = if overwrite { Resolution::Replace } else { self.ask_conflict(src, &dst, false, dst_dir) };
                    match r {
                        Resolution::Skip => {
                            self.skipped += 1;
                            self.done_bytes += size;
                            self.done_items += 1;
                            return Ok(());
                        }
                        Resolution::Replace => {
                            if dst_dir {
                                let before = self.skipped;
                                self.delete_rec(&dst, false, Some(dst.uri().to_string()))?;
                                if self.skipped != before {
                                    self.done_bytes += size;
                                    return Ok(()); // couldn't remove it, give up on this item
                                }
                                replaced = true;
                            } else {
                                flags |= gio::FileCopyFlags::OVERWRITE;
                            }
                        }
                        Resolution::KeepBoth => dst = self.keep_both(&dst, false),
                        Resolution::Cancel => return Err(Cancelled),
                    }
                }
                Err(e) => match self.ask_error(fill(&tr("Error while copying “{name}”"), &[("name", &name)]), &e, false) {
                    ErrChoice::Retry => continue,
                    ErrChoice::Cancel => return Err(Cancelled),
                    _ => {
                        self.done_bytes += size;
                        return Ok(());
                    }
                },
            }
        }
    }

    /// Whether `dst` holds what `src` does: both read (the copy from the
    /// disk or share, not from memory) and their CRC32s compared. `size`:
    /// the file's, for the progress (counted after the copy's own bytes).
    fn verify(&mut self, src: &gio::File, dst: &gio::File, size: u64) -> Result<bool, glib::Error> {
        let label = fill(&tr("Checking “{name}”"), &[("name", &base_name(src))]);
        let base = self.done_bytes + size;
        let a = self.crc32(src, false, base, &label)?;
        #[cfg(test)]
        if tests::CORRUPT_NEXT_COPY.with(|c| c.replace(false)) {
            if let Some(p) = dst.path() {
                let mut data = std::fs::read(&p).unwrap_or_default();
                if let Some(b) = data.first_mut() {
                    *b ^= 0xff;
                }
                let _ = std::fs::write(&p, data);
            }
        }
        let b = self.crc32(dst, true, base + size / 2, &label)?;
        glib::g_debug!("arkha", "check: {} {} (CRC32 {a}, copy {b})", dst.uri(), if a == b { "matches" } else { "differs" });
        Ok(a == b)
    }

    /// The CRC32 of `f`'s contents (see checksums::compute), the progress
    /// going from `base`, each byte read counting half; pausing the job
    /// pauses it.
    fn crc32(&mut self, f: &gio::File, from_disk: bool, base: u64, label: &str) -> Result<String, glib::Error> {
        let (tx, paused, cancel, clock, total) = (self.tx.clone(), self.paused.clone(), self.cancel.clone(), self.clock.clone(), self.total_bytes);
        let mut last = Instant::now();
        crate::checksums::compute(f, crate::checksums::Algo::Crc32, from_disk, &cancel, &mut |read| {
            wait_while_paused(&paused, &cancel, &clock);
            if last.elapsed() > Duration::from_millis(100) && total > 0 {
                last = Instant::now();
                let done = base + read / 2;
                let _ = tx.send_blocking(Msg::Progress {
                    detail: format!("{label}\n{}", bytes_detail(done, total, clock.working())),
                    fraction: (done as f64 / total as f64).clamp(0.0, 1.0),
                });
            }
        })
    }

    fn move_rec(&mut self, src: &gio::File, dst: &gio::File, created: &mut Option<String>) -> Flow {
        if self.cancelled() {
            return Err(Cancelled);
        }
        let mut dst = dst.clone();
        let mut flags = gio::FileCopyFlags::NOFOLLOW_SYMLINKS
            | gio::FileCopyFlags::ALL_METADATA
            | gio::FileCopyFlags::NO_FALLBACK_FOR_MOVE;
        let mut replaced = false;
        let name = base_name(src);
        loop {
            match src.move_(&dst, flags, Some(&self.cancel), None) {
                Ok(()) => {
                    self.thumbnails_moved(src, &dst);
                    self.record(src, &dst, replaced || flags.contains(gio::FileCopyFlags::OVERWRITE));
                    self.done_items += 1;
                    if created.is_none() {
                        *created = Some(dst.uri().to_string());
                    }
                    self.progress(&fill(&tr("Moving “{name}”"), &[("name", &name)]), false);
                    return Ok(());
                }
                Err(e) if e.matches(gio::IOErrorEnum::Exists) => {
                    let src_dir = kind_of(src) == Some(gio::FileType::Directory);
                    let dst_dir = kind_of(&dst) == Some(gio::FileType::Directory);
                    match self.ask_conflict(src, &dst, src_dir, dst_dir) {
                        Resolution::Skip => {
                            self.skipped += 1;
                            self.done_items += 1;
                            return Ok(());
                        }
                        Resolution::Replace => {
                            if src_dir && dst_dir {
                                // merge: move children one by one, then remove src
                                // check-placeholders: filled later
                                let Some(children) = self.list_children(src, n_("Error while reading “{name}”"))? else {
                                    return Ok(());
                                };
                                let before = self.skipped;
                                let mut none = None;
                                for c in children {
                                    self.move_rec(&src.child(&c), &dst.child(&c), &mut none)?;
                                }
                                if self.skipped == before {
                                    let _ = src.delete(gio::Cancellable::NONE);
                                }
                                if created.is_none() {
                                    *created = Some(dst.uri().to_string());
                                }
                                self.done_items += 1;
                                return Ok(());
                            }
                            if dst_dir {
                                let before = self.skipped;
                                self.delete_rec(&dst, false, Some(dst.uri().to_string()))?;
                                if self.skipped != before {
                                    return Ok(()); // couldn't remove it, keep the source
                                }
                                replaced = true;
                            } else {
                                flags |= gio::FileCopyFlags::OVERWRITE;
                            }
                        }
                        Resolution::KeepBoth => dst = self.keep_both(&dst, src_dir),
                        Resolution::Cancel => return Err(Cancelled),
                    }
                }
                Err(e)
                    if e.matches(gio::IOErrorEnum::WouldRecurse)
                        || e.matches(gio::IOErrorEnum::NotSupported)
                        || e.matches(gio::IOErrorEnum::WouldMerge) =>
                {
                    // Different filesystem: copy, then delete the source.
                    self.count_move_bytes();
                    let before = self.skipped;
                    let overwrite = flags.contains(gio::FileCopyFlags::OVERWRITE);
                    self.thumbs_of_copied.clear();
                    self.copy_rec(src, &dst, overwrite, created, true)?;
                    if self.skipped == before {
                        self.delete_rec(src, false, None)?;
                        self.thumbnails_of_deleted();
                    }
                    return Ok(());
                }
                Err(e) => match self.ask_error(fill(&tr("Error while moving “{name}”"), &[("name", &name)]), &e, false) {
                    ErrChoice::Retry => continue,
                    ErrChoice::Cancel => return Err(Cancelled),
                    _ => return Ok(()),
                },
            }
        }
    }

    fn run(&mut self, job: Job) -> Vec<String> {
        let mut created = vec![];
        match job {
            Job::Transfer { kind, items, check } => {
                self.check = check;
                self.kind = kind;
                if kind == Kind::Copy {
                    self.by_bytes = true;
                    for it in &items {
                        let (mut b, mut n) = (0, 0);
                        self.scan(&gio::File::for_uri(&it.src), &mut b, &mut n);
                        self.total_bytes += b * self.byte_passes();
                        self.total_items += n;
                    }
                } else {
                    self.total_items = items.len() as u64;
                }
                if kind == Kind::Move {
                    self.moves.srcs = items.iter().map(|it| gio::File::for_uri(&it.src)).collect();
                }
                self.progress(&tr("Preparing…"), true);
                for (at, it) in items.into_iter().enumerate() {
                    self.moves.at = at;
                    let src = gio::File::for_uri(&it.src);
                    let dest_dir = gio::File::for_uri(&it.dest_dir);
                    let src_is_dir = kind_of(&src) == Some(gio::FileType::Directory);
                    if kind != Kind::Link && (dest_dir.has_prefix(&src) || src.equal(&dest_dir) || inside_through_links(&dest_dir, &src)) {
                        let e = glib::Error::new(
                            gio::IOErrorEnum::InvalidArgument,
                            &tr("The destination is inside the source folder."),
                        );
                        // check-placeholders: filled later
                        let failed = if kind == Kind::Move { tr("Cannot move “{name}” into itself") } else { tr("Cannot copy “{name}” into itself") };
                        if self.ask_error(fill(&failed, &[("name", &base_name(&src))]), &e, false) == ErrChoice::Cancel {
                            break;
                        }
                        continue;
                    }
                    let same_dir = src.parent().map(|p| p.equal(&dest_dir)).unwrap_or(false);
                    let name = it.name.clone().unwrap_or_else(|| base_name(&src));
                    // Onto a USB stick or the like (FAT, exFAT, NTFS): names
                    // as they can hold them, and FAT's limit of 4 GB a file.
                    let fs = dest_dir
                        .query_filesystem_info("filesystem::type", Some(&self.cancel))
                        .ok()
                        .and_then(|i| i.attribute_string("filesystem::type"))
                        .map(|t| t.to_string())
                        .unwrap_or_default();
                    self.windows_names = windows_names_on(&fs);
                    self.fat32 = matches!(fs.as_str(), "vfat" | "msdos" | "fat");
                    let name = if self.windows_names && kind != Kind::Link { windows_name(&name) } else { name };
                    // Restoring from the trash: recreate the original folder if needed.
                    if it.name.is_some() && !dest_dir.query_exists(gio::Cancellable::NONE) {
                        let _ = dest_dir.make_directory_with_parents(gio::Cancellable::NONE);
                    }
                    let mut c = None;
                    let r = match kind {
                        Kind::Copy => {
                            let name = if same_dir { util::copy_name(&dest_dir, &name, src_is_dir) } else { name };
                            self.copy_rec(&src, &dest_dir.child(&name), false, &mut c, true)
                        }
                        Kind::Move => {
                            if same_dir && it.name.is_none() {
                                self.done_items += 1;
                                Ok(())
                            } else {
                                self.move_rec(&src, &dest_dir.child(&name), &mut c)
                            }
                        }
                        Kind::Link => {
                            // TRANSLATORS: the name of a link made beside its target.
                            let name = if same_dir { fill(&tr("Link to {name}"), &[("name", &name)]) } else { name };
                            let name = util::unique_name(&dest_dir, &name, src_is_dir);
                            let dst = dest_dir.child(&name);
                            let target = src
                                .path()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_else(|| src.uri().to_string());
                            match dst.make_symbolic_link(target, Some(&self.cancel)) {
                                Ok(()) => {
                                    self.record(&src, &dst, false);
                                    c = Some(dst.uri().to_string());
                                    self.done_items += 1;
                                    Ok(())
                                }
                                Err(e) => match self.ask_error(fill(&tr("Cannot create link to “{name}”"), &[("name", &name)]), &e, false) {
                                    ErrChoice::Cancel => Err(Cancelled),
                                    _ => Ok(()),
                                },
                            }
                        }
                    };
                    if let Some(c) = c {
                        created.push(c);
                    }
                    // Counted in bytes: this item is done, whether it was
                    // copied, renamed or skipped (in part).
                    if let Some(sizes) = &self.moves.sizes {
                        self.done_bytes = sizes.iter().take(at + 1).sum();
                    }
                    if r.is_err() {
                        break;
                    }
                }
            }
            Job::Trash(uris) => {
                self.total_items = uris.len() as u64;
                self.progress(&tr("Preparing…"), true);
                for u in uris {
                    let f = gio::File::for_uri(&u);
                    loop {
                        if self.cancelled() {
                            break;
                        }
                        match f.trash(Some(&self.cancel)) {
                            Ok(()) => {
                                // For Undo: brought back out of the trash.
                                self.undo.push(Moved { src: u, dst: String::new(), replaced: false });
                                self.done_items += 1;
                                self.progress(&fill(&tr("Trashing “{name}”"), &[("name", &base_name(&f))]), false);
                                break;
                            }
                            Err(e) => match self.ask_error(
                                fill(&tr("“{name}” can’t be put in the trash. Delete it permanently?"), &[("name", &base_name(&f))]),
                                &e,
                                true,
                            ) {
                                ErrChoice::Retry => continue,
                                ErrChoice::DeletePermanently => {
                                    let _ = self.delete_rec(&f, false, Some(u));
                                    break;
                                }
                                _ => break,
                            },
                        }
                    }
                    if self.cancelled() {
                        break;
                    }
                }
            }
            Job::Delete(uris) => {
                for u in &uris {
                    let (mut b, mut n) = (0, 0);
                    self.scan(&gio::File::for_uri(u), &mut b, &mut n);
                    self.total_items += n;
                }
                self.progress(&tr("Preparing…"), true);
                for u in uris {
                    let f = gio::File::for_uri(&u);
                    let thumb = thumb_uri(&f);
                    if self.delete_rec(&f, false, Some(thumb)).is_err() {
                        break;
                    }
                }
            }
            Job::EmptyTrash => {
                let trash = gio::File::for_uri("trash:///");
                let kids: Vec<(gio::File, Option<String>)> = trash
                    .enumerate_children("standard::name,trash::orig-path", NOFOLLOW, Some(&self.cancel))
                    .map(|en| {
                        let mut v = vec![];
                        while let Ok(Some(ci)) = en.next_file(Some(&self.cancel)) {
                            let orig = ci.attribute_byte_string("trash::orig-path").map(|p| gio::File::for_path(p.as_str()).uri().to_string());
                            v.push((trash.child(ci.name()), orig));
                        }
                        v
                    })
                    .unwrap_or_default();
                self.total_items = kids.len() as u64;
                self.progress(&tr("Preparing…"), true);
                for (k, orig) in kids {
                    if self.delete_rec(&k, false, orig).is_err() {
                        break;
                    }
                }
            }
        }
        created
    }
}

// ---------------------------------------------------------------------------
// UI side
//
// Every running or waiting operation has one shared `Op`. Each place that
// lists operations (a window's operations popover, the File Operations
// window) is a `View` with rows of its own, built from and kept in step
// with the `Op`s, so all of them can be open at once.

#[derive(Clone, Copy, PartialEq, Eq)]
enum OpState {
    /// In the queue: Start Now or Cancel.
    Waiting,
    /// Has a worker: progress, Pause and Cancel.
    Running,
}

struct Op {
    id: u64,
    title: String,
    state: OpState,
    detail: RefCell<String>,
    fraction: std::cell::Cell<f64>,
    paused: Arc<AtomicBool>,
    cancel: gio::Cancellable,
    cancelled: std::cell::Cell<bool>,
}

/// The widgets of one operation's row in one view.
struct Row {
    row: gtk::Box,
    detail: gtk::Label,
    bar: gtk::ProgressBar,
    pause: gtk::ToggleButton,
    pause_img: gtk::Image,
    cancel: gtk::Button,
}

struct View {
    holder: glib::WeakRef<gtk::Box>,
    rows: RefCell<Vec<(u64, Row)>>,
}

/// The progress line under the bar, marked while the job is paused.
fn progress_text(detail: &str, paused: bool) -> String {
    if paused {
        fill(&tr("Paused — {progress}"), &[("progress", &detail.replace('\n', " — "))])
    } else {
        detail.to_string()
    }
}

thread_local! {
    static RUNNING: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Whether an operation since the queue last ran empty took a while
    /// (and was listed): only then is its end told, as in Caja.
    static LISTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The operations shown, in order.
    static OPS: RefCell<Vec<Rc<Op>>> = const { RefCell::new(vec![]) };
    static VIEWS: RefCell<Vec<Rc<View>>> = const { RefCell::new(vec![]) };
    static OPS_WINDOW: RefCell<Option<gtk::Window>> = const { RefCell::new(None) };
    static LISTENERS: RefCell<Vec<Box<dyn Fn()>>> = RefCell::new(vec![]);
    static MOVE_LISTENERS: RefCell<Vec<Box<WentFn>>> = RefCell::new(vec![]);
}

/// A new list of the operations (for a window's operations popover); it
/// stays up to date by itself.
pub fn new_view() -> gtk::Box {
    let holder = gtk::Box::new(gtk::Orientation::Vertical, 12);
    holder.set_border_width(12);
    let view = Rc::new(View { holder: holder.downgrade(), rows: RefCell::new(vec![]) });
    VIEWS.with(|v| v.borrow_mut().push(view.clone()));
    sync_view(&view);
    holder
}

/// Shows the operations in their own window.
pub fn open_window() {
    let win = OPS_WINDOW.with(|w| {
        w.borrow_mut()
            .get_or_insert_with(|| {
                let win = gtk::Window::new(gtk::WindowType::Toplevel);
                win.set_title(&tr("File Operations"));
                win.set_default_size(440, -1);
                win.set_resizable(false);
                win.set_icon_name(Some(crate::util::app_icon()));
                win.set_type_hint(gdk::WindowTypeHint::Dialog);
                win.connect_delete_event(|w, _| {
                    w.hide();
                    glib::Propagation::Stop
                });
                win.add(&new_view());
                win
            })
            .clone()
    });
    win.show_all();
    win.present();
}

/// Told of items an operation took from where they were (see on_moved).
type WentFn = dyn Fn(&[(String, Option<String>)]);

/// Whether a file system of type `fs` (GIO's filesystem::type) keeps names
/// as Windows does: USB sticks and drives formatted for Windows.
fn windows_names_on(fs: &str) -> bool {
    matches!(fs, "vfat" | "msdos" | "fat" | "exfat" | "ntfs" | "ntfs3" | "fuseblk")
}

/// `name` as a drive formatted for Windows can hold it, as Nautilus makes
/// them: the characters it doesn't allow (" * : < > ? \ | and control
/// characters) made "_", and no space or dot at the end. Such names failed
/// one by one.
fn windows_name(name: &str) -> String {
    let n: String = name.chars().map(|c| if c < ' ' || "\"*:<>?\\|".contains(c) { '_' } else { c }).collect();
    let n = n.trim_end_matches([' ', '.']);
    if n.is_empty() { "_".into() } else { n.to_string() }
}

/// Whether `dest_dir` is `src` or inside it once links are followed (a
/// link in the way, the addresses don't show it): a folder copied into a
/// link to its own subfolder copied itself into itself, some 2,000 levels
/// deep, until the path was too long.
fn inside_through_links(dest_dir: &gio::File, src: &gio::File) -> bool {
    let real = |f: &gio::File| f.path().and_then(|p| std::fs::canonicalize(p).ok());
    match (real(dest_dir), real(src)) {
        (Some(d), Some(s)) => d.starts_with(&s),
        _ => false,
    }
}

/// The URI `f`'s thumbnails are kept under: for an item in the trash,
/// where it came from (trashed, it kept them).
fn thumb_uri(f: &gio::File) -> String {
    if f.uri_scheme().as_deref() == Some("trash") {
        if let Some(p) = f.query_info("trash::orig-path", NOFOLLOW, gio::Cancellable::NONE).ok().and_then(|i| i.attribute_byte_string("trash::orig-path")) {
            return gio::File::for_path(p.as_str()).uri().to_string();
        }
    }
    f.uri().to_string()
}

/// Calls `f` when an operation has taken items from where they were, with
/// (each one's URI before, its new one if it was moved, None if it was
/// trashed or deleted). A folder's monitor tells its pane; search results
/// have nothing watching them.
pub fn on_moved(f: impl Fn(&[(String, Option<String>)]) + 'static) {
    MOVE_LISTENERS.with(|l| l.borrow_mut().push(Box::new(f)));
}

/// What `job` takes from where it was (moves, trashing, deleting), and
/// whether it's a move.
fn taken_by(job: &Job) -> (Vec<String>, bool) {
    match job {
        Job::Transfer { kind: Kind::Move, items, .. } => (items.iter().map(|i| i.src.clone()).collect(), true),
        Job::Trash(v) | Job::Delete(v) => (v.clone(), false),
        _ => (vec![], false),
    }
}

/// Which of `taken` (see taken_by) went: (URI before, new URI for a move
/// as `done` has it). Items `done` doesn't name are gone if they're no
/// longer there (`gone` tells; it's asked only of local files, quick to
/// look at: one that couldn't be deleted stays).
fn went(taken: &[String], moves: bool, done: &[Moved], gone: impl Fn(&str) -> bool) -> Vec<(String, Option<String>)> {
    let moved: std::collections::HashSet<&str> = if moves { done.iter().map(|m| m.src.as_str()).collect() } else { Default::default() };
    let mut out: Vec<(String, Option<String>)> = done.iter().filter(|_| moves).map(|m| (m.src.clone(), Some(m.dst.clone()))).collect();
    out.extend(taken.iter().filter(|t| !moved.contains(t.as_str()) && gone(t)).map(|t| (t.clone(), None)));
    out
}

/// Calls `f` whenever operations come, go or make progress.
pub fn on_change(f: impl Fn() + 'static) {
    LISTENERS.with(|l| l.borrow_mut().push(Box::new(f)));
}

/// Closes the File Operations window once nothing is running or waiting.
/// (Not merely when the list is empty: a new operation only joins it after
/// a moment, and the window may have been opened for it already.)
fn hide_window_if_idle() {
    if !is_busy() && OPS.with(|o| o.borrow().is_empty()) {
        if let Some(w) = OPS_WINDOW.with(|w| w.borrow().clone()) {
            w.hide();
        }
    }
}

fn changed() {
    LISTENERS.with(|l| {
        for f in l.borrow().iter() {
            f();
        }
    });
}

/// (operations shown, overall progress of the running ones).
pub fn summary() -> (usize, f64) {
    OPS.with(|o| {
        let o = o.borrow();
        let running: Vec<f64> = o.iter().filter(|op| op.state == OpState::Running).map(|op| op.fraction.get()).collect();
        let frac = if running.is_empty() { 0.0 } else { running.iter().sum::<f64>() / running.len() as f64 };
        (o.len(), frac)
    })
}

/// Where a new entry goes in the list: running jobs above the waiting ones,
/// each in the order they came. (A queued job that starts gets a running
/// entry of its own; at the end it was listed below the jobs still
/// waiting, which then looked first in line.)
fn insert_at(states: &[OpState], new: OpState) -> usize {
    match new {
        OpState::Waiting => states.len(),
        OpState::Running => states.iter().position(|s| *s == OpState::Waiting).unwrap_or(states.len()),
    }
}

fn add_op(op: Rc<Op>) {
    // Operations in their own window (a setting): it opens with the first
    // one listed, also again if it was closed meanwhile.
    if crate::config::with(|s| s.ops_window) && !OPS_WINDOW.with(|w| w.borrow().as_ref().is_some_and(|w| w.is_visible())) {
        glib::idle_add_local_once(open_window);
    }
    OPS.with(|o| {
        let mut o = o.borrow_mut();
        let states: Vec<OpState> = o.iter().map(|op| op.state).collect();
        o.insert(insert_at(&states, op.state), op);
    });
    sync_views();
    changed();
}

fn remove_op(id: u64) {
    OPS.with(|o| o.borrow_mut().retain(|op| op.id != id));
    sync_views();
    hide_window_if_idle();
    changed();
}

/// Brings every view's rows in line with OPS (adds, removes, reorders).
fn sync_views() {
    let views = VIEWS.with(|v| {
        // Views whose window is gone drop out.
        v.borrow_mut().retain(|view| view.holder.upgrade().is_some());
        v.borrow().clone()
    });
    for view in &views {
        sync_view(view);
    }
}

fn sync_view(view: &View) {
    let Some(holder) = view.holder.upgrade() else { return };
    let ops = OPS.with(|o| o.borrow().clone());
    let mut rows = view.rows.borrow_mut();
    rows.retain(|(id, r)| {
        let keep = ops.iter().any(|op| op.id == *id);
        if !keep {
            holder.remove(&r.row);
        }
        keep
    });
    for (pos, op) in ops.iter().enumerate() {
        if !rows.iter().any(|(id, _)| *id == op.id) {
            let r = build_row(op);
            holder.pack_start(&r.row, false, false, 0);
            r.row.show_all();
            update_row(op, &r);
            rows.push((op.id, r));
        }
        if let Some((_, r)) = rows.iter().find(|(id, _)| *id == op.id) {
            holder.reorder_child(&r.row, pos as i32);
        }
    }
}

/// Refreshes `op`'s rows in every view (progress, pause and cancel state).
fn update_op(op: &Op) {
    let views = VIEWS.with(|v| v.borrow().clone());
    for view in views {
        if let Some((_, r)) = view.rows.borrow().iter().find(|(id, _)| *id == op.id) {
            update_row(op, r);
        }
    }
}

fn update_row(op: &Op, r: &Row) {
    if op.state == OpState::Waiting {
        return;
    }
    let paused = op.paused.load(Ordering::Relaxed);
    r.detail.set_text(&progress_text(&op.detail.borrow(), paused));
    r.bar.set_fraction(op.fraction.get());
    // Only when it differs: setting it fires toggled, which calls set_paused.
    if r.pause.is_active() != paused {
        r.pause.set_active(paused);
    }
    let icon = if paused { "media-playback-start-symbolic" } else { "media-playback-pause-symbolic" };
    r.pause_img.set_from_icon_name(Some(icon), gtk::IconSize::Button);
    r.pause.set_tooltip_text(Some(&if paused { tr("Resume") } else { tr("Pause") }));
    // A cancelled job can't be paused or cancelled any more.
    r.pause.set_sensitive(!op.cancelled.get());
    r.cancel.set_sensitive(!op.cancelled.get());
}

fn set_paused(op: &Op, on: bool) {
    if op.paused.load(Ordering::Relaxed) != on {
        op.paused.store(on, Ordering::Relaxed);
        update_op(op);
    }
}

fn build_row(op: &Rc<Op>) -> Row {
    let r = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let hb = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let detail = gtk::Label::new(None);
    detail.set_xalign(0.0);
    detail.set_ellipsize(pango::EllipsizeMode::Middle);
    detail.style_context().add_class("dim-label");
    let bar = gtk::ProgressBar::new();
    let pause_img = gtk::Image::from_icon_name(Some("media-playback-pause-symbolic"), gtk::IconSize::Button);
    let pause = gtk::ToggleButton::new();
    pause.add(&pause_img);
    let cancel = gtk::Button::from_icon_name(Some("process-stop-symbolic"), gtk::IconSize::Button);
    cancel.set_tooltip_text(Some(&tr("Cancel")));
    let id = op.id;
    match op.state {
        OpState::Waiting => {
            detail.set_text(&tr("Waiting for other operations…"));
            detail.set_hexpand(true);
            let go = gtk::Button::from_icon_name(Some("media-skip-forward-symbolic"), gtk::IconSize::Button);
            go.set_tooltip_text(Some(&tr("Start now, alongside the running operations")));
            go.connect_clicked(move |_| {
                if let Some(w) = take_waiting(id) {
                    (w.start)();
                }
            });
            cancel.connect_clicked(move |_| {
                take_waiting(id);
            });
            hb.pack_start(&detail, true, true, 0);
            hb.pack_start(&go, false, false, 0);
            hb.pack_start(&cancel, false, false, 0);
            r.pack_start(&bold_label(&op.title), false, false, 0);
            r.pack_start(&hb, false, false, 0);
        }
        OpState::Running => {
            bar.set_hexpand(true);
            bar.set_valign(gtk::Align::Center);
            {
                let op = Rc::downgrade(op);
                pause.connect_toggled(move |b| {
                    if let Some(op) = op.upgrade() {
                        set_paused(&op, b.is_active());
                    }
                });
            }
            {
                let op = Rc::downgrade(op);
                cancel.connect_clicked(move |_| {
                    if let Some(op) = op.upgrade() {
                        op.cancel.cancel();
                        op.cancelled.set(true);
                        update_op(&op);
                    }
                });
            }
            hb.pack_start(&bar, true, true, 0);
            hb.pack_start(&pause, false, false, 0);
            hb.pack_start(&cancel, false, false, 0);
            r.pack_start(&bold_label(&op.title), false, false, 0);
            r.pack_start(&hb, false, false, 0);
            r.pack_start(&detail, false, false, 0);
        }
    }
    Row { row: r, detail, bar, pause, pause_img, cancel }
}

/// Whether any operation is running or waiting in the queue.
pub fn is_busy() -> bool {
    RUNNING.with(|r| r.get() > 0) || QUEUE.with(|q| !q.borrow().is_empty())
}

pub fn title_for(job: &Job) -> String {
    match job {
        Job::Transfer { kind, items, .. } => {
            let n = items.len();
            // TRANSLATORS: {what} is a name in quotes or a number of items: "3 items".
            let what = if n == 1 {
                format!("“{}”", gio::File::for_uri(&items[0].src).basename().map(|b| b.to_string_lossy().to_string()).unwrap_or_default())
            } else {
                fill(&trn("{n} item", "{n} items", n as u64), &[("n", &n)])
            };
            let to = items
                .first()
                .map(|i| util::display_name(&gio::File::for_uri(&i.dest_dir)))
                .unwrap_or_default();
            match kind {
                Kind::Copy => fill(&tr("Copying {what} to “{folder}”"), &[("what", &what), ("folder", &to)]),
                Kind::Move => fill(&tr("Moving {what} to “{folder}”"), &[("what", &what), ("folder", &to)]),
                Kind::Link => fill(&tr("Creating links to {what}"), &[("what", &what)]),
            }
        }
        Job::Trash(v) => fill(&trn("Moving {n} item to the Trash", "Moving {n} items to the Trash", v.len() as u64), &[("n", &v.len())]),
        Job::Delete(v) => fill(&trn("Deleting {n} item", "Deleting {n} items", v.len() as u64), &[("n", &v.len())]),
        Job::EmptyTrash => tr("Emptying the Trash"),
    }
}

/// A copy or move waiting for the operations before it to finish.
struct Waiting {
    id: u64,
    start: Box<dyn FnOnce()>,
    /// Keeps the app running for it, like a running job does.
    _hold: Option<gio::ApplicationHoldGuard>,
}

thread_local! {
    static QUEUE: RefCell<std::collections::VecDeque<Waiting>> = RefCell::new(Default::default());
    static NEXT_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn next_id() -> u64 {
    NEXT_ID.with(|n| {
        n.set(n.get() + 1);
        n.get()
    })
}

/// Run a job. `on_done` receives the URIs of top-level items created.
/// Copies and moves wait their turn while other operations run or wait
/// (their row has a Start Now button); everything else starts right away.
pub fn run(parent: Option<gtk::Window>, job: Job, on_done: impl Fn(Vec<String>) + 'static) {
    run_with(parent, job, Record::New, on_done);
}

/// run, with where its result goes for Undo and Redo.
fn run_with(parent: Option<gtk::Window>, job: Job, record: Record, on_done: impl Fn(Vec<String>) + 'static) {
    let queueable = matches!(job, Job::Transfer { kind: Kind::Copy | Kind::Move, .. });
    if queueable && is_busy() {
        enqueue(parent, job, record, Box::new(on_done));
    } else {
        start(parent, job, record, Box::new(on_done), false);
    }
}

fn enqueue(parent: Option<gtk::Window>, job: Job, record: Record, on_done: Box<dyn Fn(Vec<String>)>) {
    let id = next_id();
    let op = Rc::new(Op {
        id,
        title: title_for(&job),
        state: OpState::Waiting,
        detail: RefCell::new(String::new()),
        fraction: std::cell::Cell::new(0.0),
        paused: Arc::new(AtomicBool::new(false)),
        cancel: gio::Cancellable::new(),
        cancelled: std::cell::Cell::new(false),
    });
    let hold = gio::Application::default().map(|a| a.hold());
    QUEUE.with(|q| {
        q.borrow_mut().push_back(Waiting {
            id,
            start: Box::new(move || start(parent, job, record, on_done, true)),
            _hold: hold,
        })
    });
    add_op(op);
}

/// Takes waiting job `id` out of the queue (and off the lists).
fn take_waiting(id: u64) -> Option<Waiting> {
    let w = QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        let i = q.iter().position(|w| w.id == id)?;
        q.remove(i)
    })?;
    remove_op(id);
    Some(w)
}

/// Starts the oldest waiting job once nothing is running.
fn start_next() {
    if RUNNING.with(|r| r.get()) > 0 {
        return;
    }
    let next = QUEUE.with(|q| q.borrow_mut().pop_front());
    if let Some(w) = next {
        // Started before its waiting entry goes: in between nothing would be
        // running, waiting or listed, and the File Operations window, kept
        // open by the user, closed as the next job started.
        (w.start)();
        remove_op(w.id);
    }
}

fn bold_label(text: &str) -> gtk::Label {
    let t = gtk::Label::new(Some(text));
    t.set_xalign(0.0);
    t.set_ellipsize(pango::EllipsizeMode::Middle);
    t.set_max_width_chars(60);
    let attrs = pango::AttrList::new();
    attrs.insert(pango::AttrInt::new_weight(pango::Weight::Bold));
    t.set_attributes(Some(&attrs));
    t
}

/// `show_now`: list the operation at once (a job leaving the queue) instead
/// of only once it has taken a moment.
fn start(parent: Option<gtk::Window>, job: Job, record: Record, on_done: Box<dyn Fn(Vec<String>)>, show_now: bool) {
    let (tx, rx) = async_channel::unbounded::<Msg>();
    let shape = shape_of(&job);
    let (taken, moves) = taken_by(&job);
    // Trashed folders keep their remembered views (for a restore).
    let trashing = matches!(job, Job::Trash(_));
    let op = Rc::new(Op {
        id: next_id(),
        title: title_for(&job),
        state: OpState::Running,
        detail: RefCell::new(tr("Preparing…")),
        fraction: std::cell::Cell::new(0.0),
        paused: Arc::new(AtomicBool::new(false)),
        cancel: gio::Cancellable::new(),
        cancelled: std::cell::Cell::new(false),
    });
    {
        let cancel = op.cancel.clone();
        let paused = op.paused.clone();
        std::thread::spawn(move || {
            let mut w = Worker::new(tx.clone(), cancel, paused);
            let created = w.run(job);
            if w.check {
                let _ = tx.send_blocking(Msg::Checked(w.checked, std::mem::take(&mut w.check_failed)));
            }
            let _ = tx.send_blocking(Msg::Done(created, std::mem::take(&mut w.undo)));
        });
    }
    RUNNING.with(|r| r.set(r.get() + 1));
    // The progress window isn't an application window, so keep the app (and
    // the worker thread) alive ourselves if every main window gets closed.
    let hold = gio::Application::default().map(|a| a.hold());

    // List the operation only if it takes a while; one from the queue (it
    // was listed) at once: listed a moment later, the list was empty in
    // between, and the operations popover, open to follow the queue,
    // closed (as the button hid).
    let finished = Rc::new(std::cell::Cell::new(false));
    if show_now {
        LISTED.with(|l| l.set(true));
        add_op(op.clone());
    } else {
        let (op, finished) = (op.clone(), finished.clone());
        glib::timeout_add_local_once(Duration::from_millis(600), move || {
            if !finished.get() {
                LISTED.with(|l| l.set(true));
                add_op(op);
            }
        });
    }

    glib::MainContext::default().spawn_local(async move {
        let parent = parent;
        let mut record = record;
        let _hold = hold;
        while let Ok(msg) = rx.recv().await {
            match msg {
                Msg::Progress { detail, fraction } => {
                    *op.detail.borrow_mut() = detail;
                    op.fraction.set(fraction);
                    update_op(&op);
                    changed();
                }
                Msg::Conflict(req, reply) => {
                    let p = dialog_parent(&parent);
                    let res = conflict_dialog(p.as_ref(), &req).await;
                    let _ = reply.send(res);
                }
                Msg::Error { title, detail, offer_delete, reply } => {
                    let p = dialog_parent(&parent);
                    let c = error_dialog(p.as_ref(), &title, &detail, offer_delete).await;
                    let _ = reply.send(c);
                }
                Msg::Checked(matched, failed) => {
                    let p = dialog_parent(&parent);
                    check_report(p.as_ref(), matched, &failed);
                }
                Msg::Done(created, done) => {
                    let did = !done.is_empty();
                    let went = went(&taken, moves, &done, |uri| {
                        let f = gio::File::for_uri(uri);
                        f.is_native() && !f.query_exists(gio::Cancellable::NONE)
                    });
                    match std::mem::replace(&mut record, Record::Redo) {
                        Record::New => {
                            if let Some(a) = action_of(shape, done) {
                                push_new(a);
                            }
                        }
                        Record::Undo(a) if did => push(&REDO, a),
                        Record::Undo(_) => {}
                        Record::Redo => {
                            if let Some(a) = action_of(shape, done) {
                                push(&UNDO, a);
                            }
                        }
                    }
                    RUNNING.with(|r| r.set(r.get().saturating_sub(1)));
                    finished.set(true);
                    // Also hides the window if this was the last operation.
                    remove_op(op.id);
                    on_done(created);
                    if !went.is_empty() && !trashing {
                        crate::folders::follow(&went);
                    }
                    if !went.is_empty() {
                        MOVE_LISTENERS.with(|l| {
                            for f in l.borrow().iter() {
                                f(&went);
                            }
                        });
                    }
                    start_next();
                    if !is_busy() && LISTED.with(|l| l.replace(false)) {
                        notify_done(&parent);
                    }
                    break;
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Undo and redo

/// Something done that Edit → Undo can take back and Redo do again, as
/// in Caja.
#[derive(Clone, Debug)]
enum Action {
    /// A copy, move or link: what went where.
    Transfer { kind: Kind, items: Vec<Moved> },
    /// Items moved to the trash (their URIs then), at about `since` (Unix
    /// time): undone by finding them in the trash.
    Trash { uris: Vec<String>, since: i64 },
    /// Items restored from the trash to `uris`; once undone (trashed
    /// again), found in the trash from `since` on to redo it.
    Restore { uris: Vec<String>, since: i64 },
    /// Renames (old URI, new URI), in the order done.
    Rename { items: Vec<(String, String)> },
    /// A new folder, empty document or one from a template; `named` once
    /// the name typed right after making it is part of it.
    Create { uri: String, folder: bool, template: Option<String>, named: bool },
    /// Permissions changed: (URI, mode before, mode after).
    Mode { items: Vec<(String, u32, u32)> },
    /// Owner or group changed: (URI, (uid, gid) before, after).
    Chown { items: Vec<(String, Ids, Ids)> },
}

/// A file's owner and group: (uid, gid).
pub type Ids = (u32, u32);

/// What a job's result goes to: a new Undo step (and the Redo list
/// forgotten), the Redo list (`Undo`: the action just undone), or an Undo
/// step without forgetting Redo (`Redo`).
enum Record {
    New,
    Undo(Action),
    Redo,
}

/// The kind of action a job is, as far as Undo goes (see action_of).
#[derive(Clone, Copy)]
enum Shape {
    Transfer(Kind),
    Restore,
    Trash(i64),
    Other,
}

fn shape_of(job: &Job) -> Shape {
    match job {
        Job::Transfer { kind: Kind::Move, items, .. } if !items.is_empty() && items.iter().all(|i| i.src.starts_with("trash:")) => Shape::Restore,
        Job::Transfer { kind, items, .. } if items.iter().all(|i| !i.src.starts_with("trash:")) => Shape::Transfer(*kind),
        Job::Trash(_) => Shape::Trash(now() - 1),
        _ => Shape::Other,
    }
}

fn now() -> i64 {
    glib::DateTime::now_local().map_or(0, |d| d.to_unix())
}

/// The action a finished job did, from what its worker noted.
fn action_of(shape: Shape, done: Vec<Moved>) -> Option<Action> {
    if done.is_empty() {
        return None;
    }
    Some(match shape {
        Shape::Transfer(kind) => Action::Transfer { kind, items: done },
        Shape::Restore => Action::Restore { uris: done.into_iter().map(|m| m.dst).collect(), since: 0 },
        Shape::Trash(since) => Action::Trash { uris: done.into_iter().map(|m| m.src).collect(), since },
        Shape::Other => return None,
    })
}

/// How many operations back Undo goes.
const UNDO_STEPS: usize = 20;

thread_local! {
    static UNDO: RefCell<Vec<Action>> = const { RefCell::new(Vec::new()) };
    static REDO: RefCell<Vec<Action>> = const { RefCell::new(Vec::new()) };
}

fn push(list: &'static std::thread::LocalKey<RefCell<Vec<Action>>>, a: Action) {
    list.with(|u| {
        let mut u = u.borrow_mut();
        u.push(a);
        if u.len() > UNDO_STEPS {
            u.remove(0);
        }
    });
    changed();
}

/// Something new done: a step to undo, and nothing to redo any more.
fn push_new(a: Action) {
    REDO.with(|r| r.borrow_mut().clear());
    push(&UNDO, a);
}

/// Notes a rename done outside a job (the views, Properties).
pub fn note_rename(from: &gio::File, to: &gio::File) {
    if from.equal(to) {
        return;
    }
    thumbnails_renamed(vec![(from.uri().to_string(), to.uri().to_string())]);
    // A new item named right away (New Folder, then typing its name): one
    // step, "Creation of Folder “Photos”".
    let (from_uri, to_uri) = (from.uri().to_string(), to.uri().to_string());
    let merged = UNDO.with(|u| match u.borrow_mut().last_mut() {
        Some(Action::Create { uri, named, .. }) if *uri == from_uri && !*named => {
            *uri = to_uri.clone();
            *named = true;
            true
        }
        _ => false,
    });
    if merged {
        REDO.with(|r| r.borrow_mut().clear());
        changed();
    } else {
        push_new(Action::Rename { items: vec![(from_uri, to_uri)] });
    }
}

/// Notes renames done together (Rename All), as one step.
pub fn note_renames(items: Vec<(String, String)>) {
    if !items.is_empty() {
        thumbnails_renamed(items.clone());
        push_new(Action::Rename { items });
    }
}

/// Renamed items' thumbnails renamed too, and a renamed folder's remembered
/// view goes with it. As in a move: a file's at once, before the view shows
/// it under its new name and asks for its thumbnail, which would make it
/// again (as it did, now and then); those of everything in a folder, and
/// those of more files than are quickly done (Rename All), in a thread of
/// their own. Network places have thumbnails too (Preferences → Preview),
/// and asking one which items are folders would be a trip there each.
fn thumbnails_renamed(items: Vec<(String, String)>) {
    /// Files whose thumbnails are renamed at once, at most.
    const AT_ONCE: usize = 100;
    crate::folders::follow(&items.iter().map(|(a, b)| (a.clone(), Some(b.clone()))).collect::<Vec<_>>());
    let root = thumbcache::root();
    if items.len() <= AT_ONCE {
        thumbcache::moved_items(&root, &items);
        std::thread::spawn(move || thumbcache::moved_inside(&root, &items));
    } else {
        std::thread::spawn(move || thumbcache::moved(&root, &items));
    }
}

/// Notes a new folder or document (`template`: the file it's a copy of).
pub fn note_created(f: &gio::File, folder: bool, template: Option<String>) {
    push_new(Action::Create { uri: f.uri().to_string(), folder, template, named: false });
}

/// Notes permissions changed: (file, mode before, mode after).
pub fn note_modes(items: Vec<(gio::File, u32, u32)>) {
    let items: Vec<(String, u32, u32)> = items.into_iter().filter(|(_, a, b)| a != b).map(|(f, a, b)| (f.uri().to_string(), a, b)).collect();
    if !items.is_empty() {
        push_new(Action::Mode { items });
    }
}

/// Notes owners or groups changed: (file, (uid, gid) before, after).
pub fn note_chowns(items: Vec<(gio::File, Ids, Ids)>) {
    let items: Vec<(String, Ids, Ids)> = items.into_iter().filter(|(_, a, b)| a != b).map(|(f, a, b)| (f.uri().to_string(), a, b)).collect();
    if !items.is_empty() {
        push_new(Action::Chown { items });
    }
}

/// "Copy of “a.txt”", "Move to Trash of 3 Items": what Undo and Redo say
/// they'd take back or do again (underscores doubled for the menu).
fn action_text(a: &Action) -> String {
    let name = |uri: &str| format!("“{}”", base_name(&gio::File::for_uri(uri)).replace('_', "__"));
    // TRANSLATORS: in "_Undo Copy of 3 Items" and the like.
    let what = |uris: Vec<&str>| match uris.as_slice() {
        [one] => name(one),
        many => fill(&trn("{n} Item", "{n} Items", many.len() as u64), &[("n", &many.len())]),
    };
    // TRANSLATORS: these follow "_Undo " or "_Redo "; {what} is a name in quotes or "3 Items".
    let text = |template: &str, what: String| fill(&tr(template), &[("what", &what)]);
    match a {
        Action::Transfer { kind, items } => {
            // check-placeholders: filled later
            let template = match kind {
                Kind::Copy => n_("Copy of {what}"),
                Kind::Move => n_("Move of {what}"),
                Kind::Link => n_("Link of {what}"),
            };
            text(template, what(items.iter().map(|m| m.dst.as_str()).collect()))
        }
        // check-placeholders: filled later
        Action::Trash { uris, .. } => text(n_("Move to Trash of {what}"), what(uris.iter().map(String::as_str).collect())),
        // check-placeholders: filled later
        Action::Restore { uris, .. } => text(n_("Restore of {what}"), what(uris.iter().map(String::as_str).collect())),
        // check-placeholders: filled later
        Action::Rename { items } => text(n_("Rename of {what}"), what(items.iter().map(|(_, to)| to.as_str()).collect())),
        // check-placeholders: filled later
        Action::Create { uri, folder, .. } => text(if *folder { n_("Creation of Folder {what}") } else { n_("Creation of {what}") }, name(uri)),
        // check-placeholders: filled later
        Action::Mode { items } => text(n_("Change of Permissions of {what}"), what(items.iter().map(|i| i.0.as_str()).collect())),
        // check-placeholders: filled later
        Action::Chown { items } => text(n_("Change of Owner or Group of {what}"), what(items.iter().map(|i| i.0.as_str()).collect())),
    }
}

/// The Edit menu's label for Undo ("_Undo Copy of “a.txt”"), or None if
/// there's nothing to undo.
pub fn undo_label() -> Option<String> {
    UNDO.with(|u| u.borrow().last().map(|a| fill(&tr("_Undo {action}"), &[("action", &action_text(a))])))
}

/// The same for Redo.
pub fn redo_label() -> Option<String> {
    REDO.with(|u| u.borrow().last().map(|a| fill(&tr("_Redo {action}"), &[("action", &action_text(a))])))
}

/// Whether `uri` may still be there (only local files are checked: asking
/// a network place could take long).
fn may_exist(uri: &str) -> bool {
    gio::File::for_uri(uri).path().is_none_or(|p| p.symlink_metadata().is_ok())
}

/// Takes back the last thing done. Err: why there was nothing to do.
pub fn undo(parent: Option<gtk::Window>) -> Result<(), String> {
    let Some(a) = UNDO.with(|u| u.borrow_mut().pop()) else {
        return Err(tr("There is nothing to undo."));
    };
    changed();
    perform(parent, a, true)
}

/// Does again the last thing undone. Err: why there was nothing to do.
pub fn redo(parent: Option<gtk::Window>) -> Result<(), String> {
    let Some(a) = REDO.with(|u| u.borrow_mut().pop()) else {
        return Err(tr("There is nothing to redo."));
    };
    changed();
    perform(parent, a, false)
}

/// Undoes (`back`) or redoes `a`: through a job where files go somewhere,
/// at once otherwise.
fn perform(parent: Option<gtk::Window>, a: Action, back: bool) -> Result<(), String> {
    let record = |a: Action| if back { Record::Undo(a) } else { Record::Redo };
    match (back, a.clone()) {
        (true, Action::Transfer { kind, items }) => run_with(parent, undo_transfer(kind, items)?, record(a), |_| {}),
        (false, Action::Transfer { kind, items }) => run_with(parent, redo_transfer(kind, items)?, Record::Redo, |_| {}),
        (true, Action::Trash { uris, since }) | (false, Action::Restore { uris, since }) => {
            let job = restore_job(&uris, since)?;
            run_with(parent, job, record(a), |_| {})
        }
        (false, Action::Trash { uris, .. }) | (true, Action::Restore { uris, .. }) => {
            let uris: Vec<String> = uris.into_iter().filter(|u| may_exist(u)).collect();
            if uris.is_empty() {
                return Err(tr("Nothing to do: the items are gone."));
            }
            // Undoing a restore: found in the trash from now on to redo it.
            let a = match a {
                Action::Restore { uris, .. } => Action::Restore { uris, since: now() - 1 },
                other => other,
            };
            run_with(parent, Job::Trash(uris), if back { Record::Undo(a) } else { Record::Redo }, |_| {})
        }
        (true, Action::Create { uri, .. }) => {
            if !may_exist(&uri) {
                return Err(tr("Nothing to undo: the item is gone."));
            }
            run_with(parent, Job::Trash(vec![uri]), record(a), |_| {})
        }
        (false, Action::Create { uri, folder, template, .. }) => {
            let f = gio::File::for_uri(&uri);
            let made = if folder {
                f.make_directory(gio::Cancellable::NONE)
            } else if let Some(t) = &template {
                gio::File::for_path(t).copy(&f, gio::FileCopyFlags::NONE, gio::Cancellable::NONE, None)
            } else {
                f.create(gio::FileCreateFlags::NONE, gio::Cancellable::NONE).map(|_| ())
            };
            made.map_err(|e| e.message().to_string())?;
            push(&UNDO, a);
        }
        (_, Action::Rename { items }) => {
            // Back in the opposite order: "a → b, b → c" undone is "c → b, b → a".
            let steps: Vec<(String, String)> = if back { items.iter().rev().map(|(f, t)| (t.clone(), f.clone())).collect() } else { items };
            for (from, to) in &steps {
                let (from, to) = (gio::File::for_uri(from), gio::File::for_uri(to));
                if to.query_exists(gio::Cancellable::NONE) {
                    return Err(fill(&tr("“{name}” is already used."), &[("name", &util::display_name(&to))]));
                }
                from.set_display_name(&base_name(&to), gio::Cancellable::NONE).map_err(|e| e.message().to_string())?;
            }
            thumbnails_renamed(steps);
            push(if back { &REDO } else { &UNDO }, a);
        }
        (_, Action::Mode { items }) => {
            for (uri, before, after) in &items {
                let mode = if back { before } else { after };
                gio::File::for_uri(uri).set_attribute_uint32("unix::mode", mode & 0o7777, gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE).map_err(|e| e.message().to_string())?;
            }
            push(if back { &REDO } else { &UNDO }, a);
        }
        (_, Action::Chown { items }) => {
            for (uri, before, after) in &items {
                let (uid, gid) = if back { before } else { after };
                let path = gio::File::for_uri(uri).path().ok_or_else(|| tr("Only local files can change owner or group."))?;
                std::os::unix::fs::chown(path, Some(*uid), Some(*gid)).map_err(|e| e.to_string())?;
            }
            push(if back { &REDO } else { &UNDO }, a);
        }
    }
    Ok(())
}

/// The job that takes back a copy (its copies go to the trash, none that
/// replaced something), move (moved back) or link (the links trashed).
fn undo_transfer(kind: Kind, items: Vec<Moved>) -> Result<Job, String> {
    let items: Vec<Moved> = items.into_iter().filter(|m| may_exist(&m.dst)).collect();
    Ok(match kind {
        Kind::Copy | Kind::Link => {
            let uris: Vec<String> = items.into_iter().filter(|m| !m.replaced).map(|m| m.dst).collect();
            if uris.is_empty() {
                return Err(tr("Nothing to undo: the copies are gone or replaced other files."));
            }
            Job::Trash(uris)
        }
        Kind::Move => {
            if items.is_empty() {
                return Err(tr("Nothing to undo: the moved items are gone."));
            }
            // A target name makes the job recreate a missing folder (one
            // emptied by moving into a folder of the same name).
            let items = items
                .into_iter()
                .map(|m| {
                    let src = gio::File::for_uri(&m.src);
                    Item {
                        src: m.dst,
                        dest_dir: src.parent().map(|p| p.uri().to_string()).unwrap_or_default(),
                        name: Some(base_name(&src)),
                    }
                })
                .collect();
            Job::Transfer { kind: Kind::Move, items, check: false }
        }
    })
}

/// The job that does a copy, move or link again: from the same items to
/// the same places (under the same names for a move).
fn redo_transfer(kind: Kind, items: Vec<Moved>) -> Result<Job, String> {
    let items: Vec<Item> = items
        .into_iter()
        .filter(|m| may_exist(&m.src))
        .map(|m| {
            let dst = gio::File::for_uri(&m.dst);
            Item {
                src: m.src,
                dest_dir: dst.parent().map(|p| p.uri().to_string()).unwrap_or_default(),
                name: (kind == Kind::Move).then(|| base_name(&dst)),
            }
        })
        .collect();
    if items.is_empty() {
        return Err(tr("Nothing to redo: the items are gone."));
    }
    Ok(Job::Transfer { kind, items, check: false })
}

/// The job that brings `uris` back out of the trash: each the newest item
/// there from that place, trashed from `since` on.
fn restore_job(uris: &[String], since: i64) -> Result<Job, String> {
    let paths: Vec<String> = uris.iter().filter_map(|u| gio::File::for_uri(u).path().map(|p| p.to_string_lossy().to_string())).collect();
    let found = find_in_trash(&paths, since);
    if found.is_empty() {
        return Err(tr("Nothing to do: the items are no longer in the Trash."));
    }
    let items = found
        .into_iter()
        .map(|(trash_uri, orig)| {
            let o = gio::File::for_path(&orig);
            Item { src: trash_uri, dest_dir: o.parent().map(|p| p.uri().to_string()).unwrap_or_default(), name: Some(base_name(&o)) }
        })
        .collect();
    Ok(Job::Transfer { kind: Kind::Move, items, check: false })
}

/// The trash's items (trash URI, original path) for `paths`: for each the
/// newest put there from that path, from `since` (Unix time) on.
fn find_in_trash(paths: &[String], since: i64) -> Vec<(String, String)> {
    let trash = gio::File::for_uri("trash:///");
    let Ok(en) = trash.enumerate_children("standard::name,trash::orig-path,trash::deletion-date", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) else {
        return vec![];
    };
    let mut best: Vec<(String, String, i64)> = vec![];
    while let Ok(Some(info)) = en.next_file(gio::Cancellable::NONE) {
        let Some(orig) = info.attribute_byte_string("trash::orig-path").map(|p| p.to_string()) else { continue };
        if !paths.contains(&orig) {
            continue;
        }
        let when = info
            .attribute_string("trash::deletion-date")
            .and_then(|d| glib::DateTime::from_iso8601(&d, Some(&glib::TimeZone::local())).ok())
            .map_or(0, |d| d.to_unix());
        if when < since {
            continue;
        }
        let uri = trash.child(info.name()).uri().to_string();
        match best.iter_mut().find(|(_, o, _)| *o == orig) {
            Some(b) if b.2 < when => *b = (uri, orig, when),
            Some(_) => {}
            None => best.push((uri, orig, when)),
        }
    }
    best.into_iter().map(|(u, o, _)| (u, o)).collect()
}

/// Tells the desktop that the file operations are done, as Caja does,
/// unless the window they were started from (or the operations' own
/// window) is the active one: one closed, minimized or on another
/// workspace isn't either.
/// After a checked copy or move: a notice if every copy matched its
/// original, a message listing those that didn't.
fn check_report(parent: Option<&gtk::Window>, matched: u64, failed: &[String]) {
    if failed.is_empty() {
        let files = fill(&trn("{n} file", "{n} files", matched), &[("n", &matched)]);
        util::notify("Arkha", &fill(&tr("Checked {files}: every copy matches its original."), &[("files", &files)]));
        return;
    }
    let list = failed.iter().take(20).map(|n| format!("• {n}")).collect::<Vec<_>>().join("\n");
    let more = if failed.len() > 20 { format!("\n{}", fill(&tr("… and {n} more"), &[("n", &(failed.len() - 20))])) } else { String::new() };
    let d = gtk::MessageDialog::new(
        parent,
        gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Warning,
        gtk::ButtonsType::Close,
        &fill(&trn("{n} copy doesn’t match its original", "{n} copies don’t match their originals", failed.len() as u64), &[("n", &failed.len())]),
    );
    let kept = fill(
        &trn(
            "They were kept as they are, and the originals of moved files were kept too. {n} other file matched.",
            "They were kept as they are, and the originals of moved files were kept too. {n} other files matched.",
            matched,
        ),
        &[("n", &matched)],
    );
    d.set_secondary_text(Some(&format!("{list}{more}\n\n{kept}")));
    d.connect_response(|d, _| d.close());
    d.show();
}

fn notify_done(parent: &Option<gtk::Window>) {
    let watched = |w: &gtk::Window| w.is_visible() && w.is_active();
    if !parent.as_ref().is_some_and(watched) && !OPS_WINDOW.with(|w| w.borrow().as_ref().is_some_and(watched)) {
        util::notify("Arkha", &tr("Process completed"));
    }
}

fn dialog_parent(parent: &Option<gtk::Window>) -> Option<gtk::Window> {
    if let Some(w) = OPS_WINDOW.with(|w| w.borrow().clone()) {
        if w.is_visible() {
            return Some(w);
        }
    }
    // The window that started the job may have been closed since.
    parent.clone().filter(|p| p.is_visible())
}

async fn conflict_dialog(parent: Option<&gtk::Window>, req: &ConflictReq) -> (Resolution, bool, Option<String>) {
    let merge = req.src_is_dir && req.dest_is_dir;
    let primary = if merge {
        fill(&tr("Merge folder “{name}”?"), &[("name", &req.name)])
    } else if req.dest_is_dir {
        fill(&tr("Replace folder “{name}”?"), &[("name", &req.name)])
    } else {
        fill(&tr("Replace file “{name}”?"), &[("name", &req.name)])
    };
    let secondary = if merge {
        fill(&tr("A folder with the same name already exists in “{folder}”.\nMerging will ask before replacing any files in it that conflict."), &[("folder", &req.dest_dir)])
    } else if req.dest_is_dir {
        fill(&tr("Another folder with the same name already exists in “{folder}”.\nReplacing it will overwrite its content."), &[("folder", &req.dest_dir)])
    } else {
        fill(&tr("Another file with the same name already exists in “{folder}”.\nReplacing it will overwrite its content."), &[("folder", &req.dest_dir)])
    };
    let d = gtk::MessageDialog::new(
        parent,
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Warning,
        gtk::ButtonsType::None,
        &primary,
    );
    d.set_title(&tr("File conflict"));
    d.set_secondary_text(Some(&format!(
        "{secondary}\n\n{}",
        fill(&tr("Original: {original}\nReplace with: {new}"), &[("original", &req.dest_desc), ("new", &req.src_desc)])
    )));
    let all = gtk::CheckButton::with_label(&tr("Apply this action to all conflicts"));
    d.message_area()
        .downcast::<gtk::Box>()
        .unwrap()
        .pack_start(&all, false, false, 6);
    all.show();
    // A name of your own for the new item, as in Caja: Replace (or Merge)
    // becomes Rename once one is typed.
    let rename = gtk::Expander::with_mnemonic(&tr("Select a _new name for the destination"));
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let entry = gtk::Entry::new();
    entry.set_text(&req.name);
    entry.set_activates_default(true);
    entry.set_hexpand(true);
    let reset = gtk::Button::with_mnemonic(&tr("R_eset"));
    row.pack_start(&entry, true, true, 0);
    row.pack_start(&reset, false, false, 0);
    row.set_margin_top(6);
    rename.add(&row);
    d.message_area()
        .downcast::<gtk::Box>()
        .unwrap()
        .pack_start(&rename, false, false, 0);
    rename.show_all();
    d.add_button(&tr("_Cancel"), gtk::ResponseType::Cancel);
    d.add_button(&tr("_Skip"), gtk::ResponseType::Other(1));
    d.add_button(&tr("_Keep Both"), gtk::ResponseType::Other(2));
    let replace_label = if merge { tr("_Merge") } else { tr("_Replace") };
    let replace = d.add_button(&replace_label, gtk::ResponseType::Other(3));
    d.set_default_response(gtk::ResponseType::Other(3));
    // The typed name, if it's a usable one other than the item's own.
    let original = req.name.clone();
    let typed = move |e: &gtk::Entry| -> Option<String> {
        let t = e.text().trim().to_string();
        (!t.is_empty() && t != original && t != "." && t != ".." && !t.contains('/')).then_some(t)
    };
    {
        let (typed, all, replace) = (typed.clone(), all.clone(), replace.clone());
        entry.connect_changed(move |e| {
            let renaming = typed(e).is_some();
            if let Some(b) = replace.downcast_ref::<gtk::Button>() {
                b.set_label(&if renaming { tr("_Rename") } else { replace_label.clone() });
            }
            // A name is for this item only.
            all.set_sensitive(!renaming);
            if renaming {
                all.set_active(false);
            }
        });
    }
    {
        let (entry, name) = (entry.clone(), req.name.clone());
        reset.connect_clicked(move |_| entry.set_text(&name));
    }
    {
        // When opened: the name selected without its extension.
        let (entry, name, is_dir) = (entry.clone(), req.name.clone(), req.src_is_dir);
        rename.connect_expanded_notify(move |x| {
            if x.is_expanded() {
                entry.grab_focus();
                let stem = if is_dir { name.as_str() } else { util::split_ext(&name).0 };
                entry.select_region(0, stem.chars().count() as i32);
            }
        });
    }
    let resp = d.run_future().await;
    let apply_all = all.is_active();
    let name = typed(&entry);
    d.close();
    match resp {
        gtk::ResponseType::Other(1) => (Resolution::Skip, apply_all, None),
        gtk::ResponseType::Other(2) => (Resolution::KeepBoth, apply_all, None),
        // Rename: Keep Both, under the typed name.
        gtk::ResponseType::Other(3) if name.is_some() => (Resolution::KeepBoth, false, name),
        gtk::ResponseType::Other(3) => (Resolution::Replace, apply_all, None),
        _ => (Resolution::Cancel, false, None),
    }
}

async fn error_dialog(parent: Option<&gtk::Window>, title: &str, detail: &str, offer_delete: bool) -> ErrChoice {
    let d = gtk::MessageDialog::new(
        parent,
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Error,
        gtk::ButtonsType::None,
        title,
    );
    d.set_secondary_text(Some(detail));
    d.add_button(&tr("_Cancel"), gtk::ResponseType::Cancel);
    d.add_button(&tr("S_kip All"), gtk::ResponseType::Other(2));
    d.add_button(&tr("_Skip"), gtk::ResponseType::Other(1));
    if offer_delete {
        d.add_button(&tr("_Delete Permanently"), gtk::ResponseType::Other(4));
    } else {
        d.add_button(&tr("_Retry"), gtk::ResponseType::Other(3));
    }
    let resp = d.run_future().await;
    d.close();
    match resp {
        gtk::ResponseType::Other(1) => ErrChoice::Skip,
        gtk::ResponseType::Other(2) => ErrChoice::SkipAll,
        gtk::ResponseType::Other(3) => ErrChoice::Retry,
        gtk::ResponseType::Other(4) => ErrChoice::DeletePermanently,
        _ => ErrChoice::Cancel,
    }
}

fn is_mtp(f: &gio::File) -> bool {
    f.uri_scheme().as_deref() == Some("mtp")
}

/// The process's umask, read without changing it (umask(2) can only be
/// read by setting it, which races with other threads creating files).
fn umask() -> u32 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find_map(|l| l.strip_prefix("Umask:").map(|v| v.trim().to_string())))
        .and_then(|v| u32::from_str_radix(&v, 8).ok())
        .unwrap_or(0o022)
}

/// Whether both are on the same phone (or camera) over MTP.
fn same_mtp_device(a: &gio::File, b: &gio::File) -> bool {
    let device = |f: &gio::File| {
        let uri = f.uri();
        uri.strip_prefix("mtp://").and_then(|r| r.split('/').next()).map(str::to_string)
    };
    device(a).is_some() && device(a) == device(b)
}

/// How much `copy_in_pieces` copies at a time: on a network share that
/// copies on the server, a fraction of a second.
const PIECE: usize = 32 * 1024 * 1024;

/// Copies the file `src` to `dst` with copy_file_range, a piece at a time,
/// reporting progress like GFile's copy does; None if that can't be used
/// here (GIO copies then). File systems that copy on the server (SMB, NFS)
/// do all of a file in one call otherwise, as GIO asks: nothing would show
/// its progress, and it couldn't be paused or cancelled until it's done.
/// Pausing happens in `progress`. With `overwrite` an existing `dst` is
/// replaced, otherwise that fails with G_IO_ERROR_EXISTS. A copy that
/// fails or is cancelled is removed.
/// A system error as GIO reports it (with its kind, as G_IO_ERROR_*).
fn io_error(e: &std::io::Error) -> glib::Error {
    io_error_with(e, &e.to_string())
}

fn io_error_with(e: &std::io::Error, message: &str) -> glib::Error {
    let code = unsafe { gio::ffi::g_io_error_from_errno(e.raw_os_error().unwrap_or(libc::EIO)) };
    let kind = unsafe { <gio::IOErrorEnum as glib::translate::FromGlib<_>>::from_glib(code) };
    glib::Error::new(kind, message)
}

/// Removes a file, link or empty folder as g_file_delete does, failing
/// alike (with G_IO_ERROR_NOT_EMPTY for a folder that isn't), but without
/// telling GVfs's metadata store: g_file_delete waits for it on every
/// file, which made deleting a big folder several times slower, and kept
/// the store busy for everything else asking it meanwhile (GTK loading an
/// icon, for one). It's told once, about the item deleted (see delete_rec):
/// it then forgets everything inside.
fn remove_path(path: &std::path::Path) -> Result<(), glib::Error> {
    let removed = std::fs::symlink_metadata(path).and_then(|m| if m.is_dir() { std::fs::remove_dir(path) } else { std::fs::remove_file(path) });
    removed.map_err(|e| {
        let reason = e.to_string();
        let reason = reason.split(" (os error").next().unwrap_or(&reason);
        io_error_with(&e, &fill(&tr("Error removing file {path}: {reason}"), &[("path", &path.display()), ("reason", &reason)]))
    })
}

fn copy_in_pieces(src: &std::path::Path, dst: &std::path::Path, overwrite: bool, cancel: &gio::Cancellable, progress: &mut dyn FnMut(i64, i64)) -> Option<Result<(), glib::Error>> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::os::unix::io::AsRawFd;
    let input = std::fs::File::open(src).ok()?;
    let meta = input.metadata().ok()?;
    // The kernel copies only within one kind of file system: don't make
    // (and remove again) a file on a network share just to find that out.
    let parent = std::ffi::CString::new(dst.parent()?.as_os_str().as_encoded_bytes()).ok()?;
    let (mut a, mut b) = unsafe { (std::mem::zeroed::<libc::statfs>(), std::mem::zeroed::<libc::statfs>()) };
    if unsafe { libc::fstatfs(input.as_raw_fd(), &mut a) != 0 || libc::statfs(parent.as_ptr(), &mut b) != 0 } || a.f_type != b.f_type {
        return None;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).mode(meta.mode() & 0o7777);
    if overwrite {
        // Cut to the new length at the end: it's left as it was if this
        // can't be used here.
        options.create(true);
    } else {
        options.create_new(true);
    }
    let output = match options.open(dst) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Some(Err(glib::Error::new(gio::IOErrorEnum::Exists, &e.to_string())));
        }
        Err(e) => return Some(Err(io_error(&e))),
    };
    let total = meta.len() as i64;
    let (mut off_in, mut off_out) = (0i64, 0i64);
    // Closed first: SMB shares refuse to delete a file that's still open
    // (EIO), and keep its name taken until it's closed.
    let fail = |output: std::fs::File, r: Result<(), glib::Error>| {
        drop(output);
        let _ = std::fs::remove_file(dst);
        Some(r)
    };
    loop {
        let n = unsafe { libc::copy_file_range(input.as_raw_fd(), &mut off_in, output.as_raw_fd(), &mut off_out, PIECE, 0) };
        if n < 0 {
            let e = std::io::Error::last_os_error();
            let unusable = matches!(e.raw_os_error(), Some(libc::EXDEV | libc::EINVAL | libc::ENOSYS | libc::EOPNOTSUPP | libc::EBADF));
            if unusable && off_out == 0 {
                // Not here: GIO copies it another way.
                drop(output);
                if !overwrite {
                    let _ = std::fs::remove_file(dst);
                }
                return None;
            }
            if e.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return fail(output, Err(io_error(&e)));
        }
        if n == 0 {
            break;
        }
        progress(off_out, total);
        if cancel.is_cancelled() {
            return fail(output, Err(glib::Error::new(gio::IOErrorEnum::Cancelled, "Cancelled")));
        }
    }
    if let Err(e) = output.set_len(off_out as u64) {
        return fail(output, Err(io_error(&e)));
    }
    Some(Ok(()))
}

/// Copies `src`'s contents to `dst` by reading and writing them, reporting
/// progress like GFile's copy does. With `overwrite` an existing `dst` is
/// replaced, otherwise that fails with G_IO_ERROR_EXISTS.
fn stream_copy(src: &gio::File, dst: &gio::File, overwrite: bool, cancel: &gio::Cancellable, progress: &mut dyn FnMut(i64, i64)) -> Result<(), glib::Error> {
    let total = src
        .query_info("standard::size", gio::FileQueryInfoFlags::NONE, Some(cancel))
        .map(|i| i.size())
        .unwrap_or(0);
    let input = src.read(Some(cancel))?;
    let output: gio::OutputStream = if overwrite {
        dst.replace(None, false, gio::FileCreateFlags::NONE, Some(cancel))?.upcast()
    } else {
        dst.create(gio::FileCreateFlags::NONE, Some(cancel))?.upcast()
    };
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0i64;
    loop {
        let n = input.read(&mut buf[..], Some(cancel))?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n], Some(cancel))?;
        done += n as i64;
        progress(done, total);
    }
    output.close(Some(cancel))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::fs;

    #[test]
    fn titles_and_what_jobs_take_away() {
        // (A folder that isn't there: its name is the last part.)
        let item = |src: &str| Item { src: src.into(), dest_dir: "file:///nowhere-arkha/Docs".into(), name: None };
        let one = vec![item("file:///a/report%20final.pdf")];
        let two = vec![item("file:///a/x"), item("file:///a/y")];
        let transfer = |kind, items: &Vec<Item>| Job::Transfer { kind, items: items.clone(), check: false };
        assert_eq!(title_for(&transfer(Kind::Copy, &one)), "Copying “report final.pdf” to “Docs”");
        assert_eq!(title_for(&transfer(Kind::Move, &two)), "Moving 2 items to “Docs”");
        assert_eq!(title_for(&transfer(Kind::Link, &one)), "Creating links to “report final.pdf”");
        assert_eq!(title_for(&Job::Trash(vec!["file:///a/x".into()])), "Moving 1 item to the Trash");
        assert_eq!(title_for(&Job::Delete(vec!["a".into(), "b".into(), "c".into()])), "Deleting 3 items");
        assert_eq!(title_for(&Job::EmptyTrash), "Emptying the Trash");
        // Moves, trashing and deleting take their items away (a move's to
        // somewhere new); copies and links leave them.
        assert_eq!(taken_by(&transfer(Kind::Move, &two)), (vec!["file:///a/x".to_string(), "file:///a/y".to_string()], true));
        assert_eq!(taken_by(&Job::Trash(vec!["t".into()])), (vec!["t".to_string()], false));
        assert_eq!(taken_by(&Job::Delete(vec!["d".into()])), (vec!["d".to_string()], false));
        assert_eq!(taken_by(&transfer(Kind::Copy, &two)), (vec![], false));
        assert_eq!(taken_by(&transfer(Kind::Link, &two)), (vec![], false));
        assert_eq!(taken_by(&Job::EmptyTrash), (vec![], false));
        // Paused: said so, the detail on one line.
        assert_eq!(progress_text("2 of 5\n1 MB/s", false), "2 of 5\n1 MB/s");
        assert_eq!(progress_text("2 of 5\n1 MB/s", true), "Paused — 2 of 5 — 1 MB/s");
    }

    thread_local! {
        /// The next copy checked is spoilt just before it's read back (in
        /// tests only), to see a mismatch caught.
        pub static CORRUPT_NEXT_COPY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    use std::path::{Path, PathBuf};

    #[test]
    fn what_went() {
        let m = |a: &str, b: &str| Moved { src: a.into(), dst: b.into(), replaced: false };
        let taken = vec!["file:///d/a".to_string(), "file:///d/b".to_string(), "file:///d/c".to_string()];
        // A move: what it did, with where to; one not moved and still
        // there (c) stays; b not done but gone (merged away) went.
        let done = [m("file:///d/a", "file:///e/a"), m("file:///d/b/x", "file:///e/b/x")];
        let went = went(&taken, true, &done, |u| u == "file:///d/b");
        assert_eq!(went, vec![
            ("file:///d/a".to_string(), Some("file:///e/a".to_string())),
            ("file:///d/b/x".to_string(), Some("file:///e/b/x".to_string())),
            ("file:///d/b".to_string(), None),
        ]);
        // Trashing or deleting: only what's no longer there.
        assert_eq!(super::went(&taken, false, &[], |u| u != "file:///d/c").len(), 2);
    }

    #[test]
    fn started_jobs_listed_above_waiting_ones() {
        use OpState::{Running as R, Waiting as W};
        assert_eq!(insert_at(&[], R), 0);
        assert_eq!(insert_at(&[R, W], W), 2);
        // The job ahead ended, the first waiting one starts: its running
        // entry comes before the one still waiting (its own waiting entry
        // is removed right after).
        assert_eq!(insert_at(&[W, W], R), 0);
        assert_eq!(insert_at(&[R, W, W], R), 1);
        assert_eq!(insert_at(&[R, R], R), 2);
    }

    /// Runs `job` on a worker whose conflict dialogs are answered from
    /// `answers` in order (with "apply to all" off) and whose error dialogs
    /// all answer Skip. Returns the created URIs and the questions asked.
    fn run_job(job: Job, answers: &[Resolution]) -> (Vec<String>, Vec<String>) {
        run_job_paused(job, answers, Arc::new(AtomicBool::new(false)))
    }

    fn run_job_paused(job: Job, answers: &[Resolution], paused: Arc<AtomicBool>) -> (Vec<String>, Vec<String>) {
        run_job_with(job, answers, paused, gio::Cancellable::new())
    }

    fn run_job_with(job: Job, answers: &[Resolution], paused: Arc<AtomicBool>, cancel: gio::Cancellable) -> (Vec<String>, Vec<String>) {
        let (created, asked, _) = run_job_full(job, answers, paused, cancel);
        (created, asked)
    }

    /// run_job, also giving what the worker noted for Undo.
    fn run_job_undo(job: Job, answers: &[Resolution]) -> (Vec<String>, Vec<Moved>) {
        let (created, _, undo) = run_job_full(job, answers, Arc::new(AtomicBool::new(false)), gio::Cancellable::new());
        (created, undo)
    }

    fn run_job_full(job: Job, answers: &[Resolution], paused: Arc<AtomicBool>, cancel: gio::Cancellable) -> (Vec<String>, Vec<String>, Vec<Moved>) {
        let answers: Vec<(Resolution, Option<String>)> = answers.iter().map(|r| (*r, None)).collect();
        run_job_answered(job, answers, paused, cancel)
    }

    /// run_job with answers that can carry a name typed for Keep Both.
    fn run_job_answered(job: Job, answers: Vec<(Resolution, Option<String>)>, paused: Arc<AtomicBool>, cancel: gio::Cancellable) -> (Vec<String>, Vec<String>, Vec<Moved>) {
        let (created, asked, undo, _) = run_job_watched(job, answers, paused, cancel);
        (created, asked, undo)
    }

    /// What run_job_watched gives: the items created, the questions asked,
    /// what undo would need, each progress report (fraction, text).
    type Watched = (Vec<String>, Vec<String>, Vec<Moved>, Vec<(f64, String)>);

    /// run_job_answered, also giving each progress report.
    fn run_job_watched(job: Job, answers: Vec<(Resolution, Option<String>)>, paused: Arc<AtomicBool>, cancel: gio::Cancellable) -> Watched {
        let (tx, rx) = async_channel::unbounded::<Msg>();
        let mut w = Worker::new(tx, cancel, paused);
        let ui = std::thread::spawn(move || {
            let (mut asked, mut progress) = (vec![], vec![]);
            let mut answers = answers.into_iter();
            while let Ok(m) = rx.recv_blocking() {
                match m {
                    Msg::Conflict(req, reply) => {
                        asked.push(format!("conflict:{}", req.name));
                        let (r, name) = answers.next().expect("unexpected conflict");
                        let _ = reply.send((r, false, name));
                    }
                    Msg::Error { title, reply, .. } => {
                        asked.push(format!("error:{title}"));
                        let _ = reply.send(ErrChoice::Skip);
                    }
                    Msg::Progress { detail, fraction } => progress.push((fraction, detail)),
                    _ => {}
                }
            }
            (asked, progress)
        });
        let created = w.run(job);
        let undo = std::mem::take(&mut w.undo);
        drop(w);
        let (asked, progress) = ui.join().unwrap();
        (created, asked, undo, progress)
    }

    /// A job run with checking, errors answered in turn by `answers`
    /// (Skip after those); `corrupt`: the first copy is spoilt before it's
    /// read back. (questions asked, files that matched, those that didn't,
    /// items noted for Undo, (bytes counted done, bytes in all)).
    fn run_checked(job: Job, answers: Vec<ErrChoice>, corrupt: bool) -> (Vec<String>, u64, Vec<String>, usize, (u64, u64)) {
        run_checked_in(job, answers, corrupt, None)
    }

    /// run_checked with thumbnails kept in `thumbs` (instead of the cache).
    fn run_checked_in(job: Job, answers: Vec<ErrChoice>, corrupt: bool, thumbs: Option<PathBuf>) -> (Vec<String>, u64, Vec<String>, usize, (u64, u64)) {
        let (tx, rx) = async_channel::unbounded::<Msg>();
        let mut w = Worker::new(tx, gio::Cancellable::new(), Arc::new(AtomicBool::new(false)));
        if let Some(t) = thumbs {
            w.thumbs = t;
        }
        let ui = std::thread::spawn(move || {
            let (mut asked, mut last) = (vec![], 0.0);
            let mut answers = answers.into_iter();
            while let Ok(m) = rx.recv_blocking() {
                match m {
                    Msg::Error { title, reply, .. } => {
                        asked.push(title);
                        let _ = reply.send(answers.next().unwrap_or(ErrChoice::Skip));
                    }
                    Msg::Conflict(req, reply) => {
                        asked.push(format!("conflict:{}", req.name));
                        let _ = reply.send((Resolution::Skip, false, None));
                    }
                    Msg::Progress { fraction, .. } => last = fraction,
                    _ => {}
                }
            }
            (asked, last)
        });
        CORRUPT_NEXT_COPY.with(|c| c.set(corrupt));
        w.run(job);
        CORRUPT_NEXT_COPY.with(|c| c.set(false));
        let (checked, failed, undo, bytes) = (w.checked, std::mem::take(&mut w.check_failed), w.undo.len(), (w.done_bytes, w.total_bytes));
        drop(w);
        let (asked, _) = ui.join().unwrap();
        (asked, checked, failed, undo, bytes)
    }

    fn checked(kind: Kind, src: &Path, dest_dir: &Path) -> Job {
        Job::Transfer { kind, items: vec![Item { src: uri(src), dest_dir: uri(dest_dir), name: None }], check: true }
    }

    #[test]
    fn checked_copies_that_match() {
        let s = home_scratch("checkok");
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 253) as u8).collect();
        fs::create_dir_all(s.0.join("src/d/sub")).unwrap();
        fs::write(s.0.join("src/d/big.bin"), &data).unwrap();
        write(&s.0.join("src/d/sub/small.txt"), "small");
        fs::create_dir_all(s.0.join("dst")).unwrap();
        let (asked, matched, failed, undo, bytes) = run_checked(checked(Kind::Copy, &s.0.join("src/d"), &s.0.join("dst")), vec![], false);
        assert!(asked.is_empty(), "{asked:?}");
        assert_eq!((matched, failed.len(), undo), (2, 0, 1));
        assert_eq!(fs::read(s.0.join("dst/d/big.bin")).unwrap(), data);
        // The checking counts in the progress too (each byte twice), and
        // it ends where it should.
        let size = data.len() as u64 + 5;
        assert_eq!(bytes, (2 * size, 2 * size));
    }

    #[test]
    fn a_spoilt_copy_is_caught_and_copied_again_or_kept() {
        let s = home_scratch("checkbad");
        write(&s.0.join("src/a.txt"), "the original");
        fs::create_dir_all(s.0.join("dst")).unwrap();
        // Skip: kept as it is, named in the report, not noted for Undo.
        let (asked, matched, failed, undo, _) = run_checked(checked(Kind::Copy, &s.0.join("src/a.txt"), &s.0.join("dst")), vec![ErrChoice::Skip], true);
        assert_eq!(asked, ["“a.txt” is different after copying"]);
        assert_eq!((matched, failed, undo), (0, vec!["a.txt".to_string()], 0));
        assert_ne!(fs::read(s.0.join("dst/a.txt")).unwrap(), b"the original");
        fs::remove_file(s.0.join("dst/a.txt")).unwrap();
        // Retry: copied again (over the spoilt one), and then it matches.
        let (asked, matched, failed, _, _) = run_checked(checked(Kind::Copy, &s.0.join("src/a.txt"), &s.0.join("dst")), vec![ErrChoice::Retry], true);
        assert_eq!(asked, ["“a.txt” is different after copying"]);
        assert_eq!((matched, failed.len()), (1, 0));
        assert_eq!(read(&s.0.join("dst/a.txt")), "the original");
    }

    #[test]
    fn a_checked_move_keeps_the_original_of_a_spoilt_copy() {
        // To another file system: the move copies (and deletes after).
        let home = home_scratch("checkmove");
        let other = scratch("/dev/shm", "checkmove");
        write(&home.0.join("a.txt"), "aaa");
        write(&home.0.join("b.txt"), "bbb");
        let job = Job::Transfer {
            kind: Kind::Move,
            items: ["a.txt", "b.txt"].iter().map(|n| Item { src: uri(&home.0.join(n)), dest_dir: uri(&other.0), name: None }).collect(),
            check: true,
        };
        let (asked, matched, failed, _, _) = run_checked(job, vec![ErrChoice::Skip], true);
        assert_eq!(asked, ["“a.txt” is different after copying"]);
        assert_eq!((matched, failed), (1, vec!["a.txt".to_string()]));
        assert!(home.0.join("a.txt").exists(), "the spoilt one's original stays");
        assert!(!home.0.join("b.txt").exists(), "the one that matched is moved");
        assert_eq!(read(&other.0.join("b.txt")), "bbb");
    }

    /// Copies onto real drives formatted for Windows, if given (as they
    /// need mounting): ARKHA_TEST_FAT, ARKHA_TEST_EXFAT, a folder on each.
    #[test]
    #[ignore = "needs ARKHA_TEST_FAT and/or ARKHA_TEST_EXFAT: folders on such drives"]
    fn copies_onto_windows_drives() {
        for (var, fat32) in [("ARKHA_TEST_FAT", true), ("ARKHA_TEST_EXFAT", false)] {
            let Ok(dir) = std::env::var(var) else { continue };
            let src = home_scratch(&format!("towindows-{fat32}"));
            write(&src.0.join("Meeting: 10:30?.txt"), "minutes");
            std::fs::create_dir_all(src.0.join("Q&A: notes")).unwrap();
            write(&src.0.join("Q&A: notes/a|b.txt"), "inside");
            let mut items = vec!["Meeting: 10:30?.txt", "Q&A: notes"];
            if fat32 {
                // 4 GB and a byte, sparse: nothing written for it here.
                std::fs::File::create(src.0.join("big.bin")).unwrap().set_len(u64::from(u32::MAX) + 2).unwrap();
                items.push("big.bin");
            }
            let dest = std::path::Path::new(&dir).join(format!("arkha-test-{}", std::process::id()));
            std::fs::create_dir_all(&dest).unwrap();
            let job = Job::Transfer { kind: Kind::Copy, items: items.iter().map(|n| Item { src: uri(&src.0.join(n)), dest_dir: uri(&dest), name: None }).collect(), check: false };
            let (_, asked) = run_job(job, &[]);
            let got = |p: &str| std::fs::read_to_string(dest.join(p)).unwrap_or_else(|e| format!("({e})"));
            assert_eq!(got("Meeting_ 10_30_.txt"), "minutes", "{var}");
            assert_eq!(got("Q&A_ notes/a_b.txt"), "inside", "{var}");
            if fat32 {
                assert_eq!(asked, ["error:“big.bin” is too big for the drive"], "{var}");
                assert!(!dest.join("big.bin").exists());
            } else {
                assert!(asked.is_empty(), "{var}: {asked:?}");
            }
            std::fs::remove_dir_all(&dest).unwrap();
        }
    }

    #[test]
    fn names_for_windows_drives() {
        assert_eq!(windows_name("Meeting: 10:30 \"draft\"?.txt"), "Meeting_ 10_30 _draft__.txt");
        assert_eq!(windows_name("a<b>c|d*e\\f"), "a_b_c_d_e_f");
        assert_eq!(windows_name("trailing dots..."), "trailing dots");
        assert_eq!(windows_name("ends with space "), "ends with space");
        assert_eq!(windows_name("tab\there"), "tab_here");
        assert_eq!(windows_name("..."), "_");
        assert_eq!(windows_name("ordinary – name (2).jpg"), "ordinary – name (2).jpg");
        assert!(windows_names_on("vfat") && windows_names_on("exfat") && windows_names_on("ntfs3"));
        assert!(!windows_names_on("ext4") && !windows_names_on("btrfs") && !windows_names_on(""));
    }

    #[test]
    fn no_copy_into_itself_through_a_link() {
        // A link to the folder's own subfolder as the destination: asked
        // about (and skipped) instead of copied into itself.
        let s = home_scratch("intoself");
        std::fs::create_dir_all(s.0.join("a/b")).unwrap();
        write(&s.0.join("a/file.txt"), "x");
        std::os::unix::fs::symlink(s.0.join("a/b"), s.0.join("link")).unwrap();
        let job = Job::Transfer { kind: Kind::Copy, items: vec![Item { src: uri(&s.0.join("a")), dest_dir: uri(&s.0.join("link")), name: None }], check: false };
        let (_, asked) = run_job(job, &[]);
        assert_eq!(asked, ["error:Cannot copy “a” into itself"]);
        assert_eq!(std::fs::read_dir(s.0.join("a/b")).unwrap().count(), 0, "nothing copied into it");
        // A link elsewhere is fine.
        std::fs::create_dir_all(s.0.join("other")).unwrap();
        std::os::unix::fs::symlink(s.0.join("other"), s.0.join("link2")).unwrap();
        let job = Job::Transfer { kind: Kind::Copy, items: vec![Item { src: uri(&s.0.join("a")), dest_dir: uri(&s.0.join("link2")), name: None }], check: false };
        assert!(run_job(job, &[]).1.is_empty());
        assert!(s.0.join("other/a/file.txt").exists());
    }

    #[test]
    fn thumbnails_in_a_move_between_drives() {
        // The copies get their thumbnails as they arrive; an original goes
        // on with its own until it's deleted. A folder with a spoilt copy
        // in it stays where it was: its files keep their thumbnails (a
        // matched one's went with its copy, and the original stayed
        // without). A file moved whole: its thumbnail goes with it.
        let home = home_scratch("thumbmove");
        let other = scratch("/dev/shm", "thumbmove");
        let cache = home.0.join("cache");
        std::fs::create_dir_all(home.0.join("dir")).unwrap();
        write(&home.0.join("dir/1-spoilt.txt"), "111");
        write(&home.0.join("dir/2-good.txt"), "222");
        write(&home.0.join("b.txt"), "bbb");
        std::fs::create_dir_all(cache.join("large")).unwrap();
        let thumb = |p: &Path| cache.join("large").join(thumbcache::name(&uri(p)));
        for n in ["dir/1-spoilt.txt", "dir/2-good.txt", "b.txt"] {
            let p = home.0.join(n);
            std::fs::write(thumb(&p), thumbcache::tests::thumb(&uri(&p))).unwrap();
        }
        let job = Job::Transfer {
            kind: Kind::Move,
            items: ["dir", "b.txt"].iter().map(|n| Item { src: uri(&home.0.join(n)), dest_dir: uri(&other.0), name: None }).collect(),
            check: true,
        };
        // The first copy checked is spoilt: which one that is depends on
        // the order the folder lists them in.
        let (_, _, failed, _, _) = run_checked_in(job, vec![ErrChoice::Skip], true, Some(cache.clone()));
        assert_eq!(failed.len(), 1);
        let (spoilt, good) = if failed[0] == "1-spoilt.txt" { ("dir/1-spoilt.txt", "dir/2-good.txt") } else { ("dir/2-good.txt", "dir/1-spoilt.txt") };
        assert!(home.0.join(good).exists(), "the folder stays");
        assert!(thumb(&home.0.join(spoilt)).exists() && thumb(&home.0.join(good)).exists(), "its files keep their thumbnails");
        assert!(!thumb(&other.0.join(spoilt)).exists(), "the spoilt copy gets none");
        assert!(thumb(&other.0.join(good)).exists(), "the good copy has one");
        assert!(thumb(&other.0.join("b.txt")).exists() && !home.0.join("b.txt").exists(), "the moved file has its thumbnail");
        assert!(!thumb(&home.0.join("b.txt")).exists(), "its original's is gone with it");
    }

    /// (dst relative to `root`, replaced) for each noted item, sorted.
    fn noted(undo: &[Moved], root: &Path) -> Vec<(String, bool)> {
        let mut v: Vec<(String, bool)> = undo
            .iter()
            .map(|m| {
                let p = gio::File::for_uri(&m.dst).path().unwrap();
                (p.strip_prefix(root).unwrap().to_string_lossy().to_string(), m.replaced)
            })
            .collect();
        v.sort();
        v
    }

    #[test]
    fn undo_notes_a_plain_copy_as_one_item() {
        let s = home_scratch("undocopy");
        write(&s.0.join("src/d/sub/c"), "c");
        write(&s.0.join("src/f"), "f");
        fs::create_dir_all(s.0.join("dst")).unwrap();
        let job = Job::Transfer {
            kind: Kind::Copy,
            items: vec![Item { src: uri(&s.0.join("src/d")), dest_dir: uri(&s.0.join("dst")), name: None }, Item { src: uri(&s.0.join("src/f")), dest_dir: uri(&s.0.join("dst")), name: None }],
            check: false,
        };
        let (_, undo) = run_job_undo(job, &[]);
        assert_eq!(noted(&undo, &s.0), [("dst/d".to_string(), false), ("dst/f".to_string(), false)]);
        // Undoing trashes just those two.
        let Ok(Job::Trash(uris)) = undo_transfer(Kind::Copy, undo) else { panic!() };
        assert_eq!(uris, [uri(&s.0.join("dst/d")), uri(&s.0.join("dst/f"))]);
    }

    #[test]
    fn undo_of_a_merging_copy_keeps_what_was_there() {
        let s = home_scratch("undocopymerge");
        let (src, dst) = merge_setup(&s.0, &s.0);
        let (_, undo) = run_job_undo(transfer(Kind::Copy, &src, &dst), &[Resolution::Replace, Resolution::Replace]);
        // Not the merged folder itself; a replaced file is noted as such.
        assert_eq!(noted(&undo, &s.0), [("dst/d/a".to_string(), true), ("dst/d/b".to_string(), false), ("dst/d/sub".to_string(), false)]);
        let Ok(Job::Trash(uris)) = undo_transfer(Kind::Copy, undo) else { panic!() };
        let mut uris = uris;
        uris.sort();
        assert_eq!(uris, [uri(&dst.join("d/b")), uri(&dst.join("d/sub"))], "the replaced a and the old x stay");
    }

    #[test]
    fn undo_skips_copies_that_are_gone() {
        let s = home_scratch("undogone");
        write(&s.0.join("src/f"), "f");
        fs::create_dir_all(s.0.join("dst")).unwrap();
        let (_, undo) = run_job_undo(transfer(Kind::Copy, &s.0.join("src/f"), &s.0.join("dst")), &[]);
        fs::remove_file(s.0.join("dst/f")).unwrap();
        assert!(undo_transfer(Kind::Copy, undo).is_err());
    }

    #[test]
    fn undo_moves_back_a_plain_move() {
        let s = home_scratch("undomove");
        write(&s.0.join("src/d/sub/c"), "c");
        fs::create_dir_all(s.0.join("dst")).unwrap();
        let (_, undo) = run_job_undo(transfer(Kind::Move, &s.0.join("src/d"), &s.0.join("dst")), &[]);
        assert_eq!(noted(&undo, &s.0), [("dst/d".to_string(), false)]);
        let job = undo_transfer(Kind::Move, undo).unwrap();
        let (_, undo) = run_job_undo(job, &[]);
        assert!(undo.len() == 1);
        assert_eq!(read(&s.0.join("src/d/sub/c")), "c");
        assert!(!s.0.join("dst/d").exists());
    }

    /// Undoing a move that merged into a folder puts back what moved in,
    /// leaves what was there before, and recreates the emptied source.
    fn undo_merge(src_root: &Path, dst_root: &Path) {
        let (src, dst) = merge_setup(src_root, dst_root);
        let (_, undo) = run_job_undo(transfer(Kind::Move, &src, &dst), &[Resolution::Replace, Resolution::Replace]);
        assert_eq!(noted(&undo, dst_root), [("dst/d/a".to_string(), true), ("dst/d/b".to_string(), false), ("dst/d/sub".to_string(), false)]);
        assert!(!src.exists());
        let job = undo_transfer(Kind::Move, undo).unwrap();
        let (_, asked) = run_job(job, &[]);
        assert!(asked.is_empty(), "{asked:?}");
        assert_eq!(read(&src.join("a")), "new a");
        assert_eq!(read(&src.join("b")), "b");
        assert_eq!(read(&src.join("sub/c")), "c");
        assert_eq!(read(&dst.join("d/x")), "x");
        assert!(!dst.join("d/a").exists() && !dst.join("d/b").exists() && !dst.join("d/sub").exists());
    }

    #[test]
    fn undo_merging_move_same_fs() {
        let s = home_scratch("undomerge");
        undo_merge(&s.0, &s.0);
    }

    #[test]
    fn undo_merging_move_cross_fs() {
        let s = home_scratch("undoxmerge");
        let t = scratch("/dev/shm", "undoxmerge");
        undo_merge(&t.0, &s.0);
    }

    /// Keep Both under a name typed in the conflict dialog.
    fn keep_both_named(kind: Kind) {
        let s = home_scratch(if kind == Kind::Copy { "namedcopy" } else { "namedmove" });
        write(&s.0.join("src/a.txt"), "new");
        write(&s.0.join("dst/a.txt"), "old");
        let job = transfer(kind, &s.0.join("src/a.txt"), &s.0.join("dst"));
        let (created, asked, undo) = run_job_answered(job, vec![(Resolution::KeepBoth, Some("b.txt".into()))], Arc::new(AtomicBool::new(false)), gio::Cancellable::new());
        assert_eq!(asked, ["conflict:a.txt"]);
        assert_eq!(read(&s.0.join("dst/a.txt")), "old");
        assert_eq!(read(&s.0.join("dst/b.txt")), "new");
        assert_eq!(created, [uri(&s.0.join("dst/b.txt"))]);
        assert_eq!(noted(&undo, &s.0), [("dst/b.txt".to_string(), false)]);
        assert_eq!(s.0.join("src/a.txt").exists(), kind == Kind::Copy);
    }

    #[test]
    fn keep_both_named_copy() {
        keep_both_named(Kind::Copy);
    }

    #[test]
    fn keep_both_named_move() {
        keep_both_named(Kind::Move);
    }

    #[test]
    fn what_a_finished_job_did() {
        let item = |src: &str, name: Option<&str>| Item { src: src.into(), dest_dir: "file:///tmp".into(), name: name.map(String::from) };
        let moved = |src: &str, dst: &str| Moved { src: src.into(), dst: dst.into(), replaced: false };
        let copy = Job::Transfer { kind: Kind::Copy, items: vec![item("file:///a", None)], check: false };
        assert!(matches!(action_of(shape_of(&copy), vec![moved("file:///a", "file:///tmp/a")]), Some(Action::Transfer { kind: Kind::Copy, .. })));
        // Nothing done: nothing to undo.
        assert!(action_of(shape_of(&copy), vec![]).is_none());
        let restore = Job::Transfer { kind: Kind::Move, items: vec![item("trash:///a", Some("a"))], check: false };
        match action_of(shape_of(&restore), vec![moved("trash:///a", "file:///tmp/a")]) {
            Some(Action::Restore { uris, .. }) => assert_eq!(uris, ["file:///tmp/a"]),
            other => panic!("{other:?}"),
        }
        let trash = Job::Trash(vec!["file:///a".into()]);
        match action_of(shape_of(&trash), vec![moved("file:///a", "")]) {
            Some(Action::Trash { uris, since }) => {
                assert_eq!(uris, ["file:///a"]);
                assert!(since > 0);
            }
            other => panic!("{other:?}"),
        }
        assert!(action_of(shape_of(&Job::Delete(vec!["file:///a".into()])), vec![moved("file:///a", "")]).is_none());
        assert!(action_of(shape_of(&Job::EmptyTrash), vec![moved("file:///a", "")]).is_none());
    }

    #[test]
    fn redoing_a_transfer() {
        let s = home_scratch("redo");
        write(&s.0.join("a.txt"), "a");
        let moved = vec![Moved { src: uri(&s.0.join("a.txt")), dst: uri(&s.0.join("dst/b.txt")), replaced: false }];
        let Ok(Job::Transfer { kind: Kind::Move, items, .. }) = redo_transfer(Kind::Move, moved.clone()) else { panic!() };
        assert_eq!((items[0].dest_dir.as_str(), items[0].name.as_deref()), (uri(&s.0.join("dst")).as_str(), Some("b.txt")));
        let Ok(Job::Transfer { kind: Kind::Copy, items, .. }) = redo_transfer(Kind::Copy, moved.clone()) else { panic!() };
        assert_eq!(items[0].name, None);
        // A source that's gone: nothing to redo.
        fs::remove_file(s.0.join("a.txt")).unwrap();
        assert!(redo_transfer(Kind::Copy, moved).is_err());
    }

    #[test]
    fn undo_and_redo_labels() {
        let a = Action::Transfer { kind: Kind::Copy, items: vec![Moved { src: "file:///a".into(), dst: "file:///b/my_file".into(), replaced: false }] };
        assert_eq!(action_text(&a), "Copy of “my__file”");
        let t = Action::Trash { uris: vec!["file:///a".into(), "file:///b".into()], since: 0 };
        assert_eq!(action_text(&t), "Move to Trash of 2 Items");
        let c = Action::Create { uri: "file:///x/New".into(), folder: true, template: None, named: false };
        assert_eq!(action_text(&c), "Creation of Folder “New”");
        let r = Action::Rename { items: vec![("file:///x/a".into(), "file:///x/b".into())] };
        assert_eq!(action_text(&r), "Rename of “b”");
    }

    #[test]
    fn undo_and_redo_renames_modes_and_new_folders() {
        use std::os::unix::fs::PermissionsExt;
        let s = home_scratch("undoredo");
        write(&s.0.join("a"), "1");
        write(&s.0.join("b"), "2");
        // A chain: b → c, then a → b. Undone in reverse, redone in order.
        let (a, b, c) = (uri(&s.0.join("a")), uri(&s.0.join("b")), uri(&s.0.join("c")));
        fs::rename(s.0.join("b"), s.0.join("c")).unwrap();
        fs::rename(s.0.join("a"), s.0.join("b")).unwrap();
        let chain = Action::Rename { items: vec![(b.clone(), c), (a, b)] };
        perform(None, chain.clone(), true).unwrap();
        assert_eq!((read(&s.0.join("a")), read(&s.0.join("b"))), ("1".to_string(), "2".to_string()));
        perform(None, chain, false).unwrap();
        assert_eq!((read(&s.0.join("b")), read(&s.0.join("c"))), ("1".to_string(), "2".to_string()));
        // Permissions back and forth.
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o7777;
        fs::set_permissions(s.0.join("b"), fs::Permissions::from_mode(0o600)).unwrap();
        let m = Action::Mode { items: vec![(uri(&s.0.join("b")), 0o644, 0o600)] };
        perform(None, m.clone(), true).unwrap();
        assert_eq!(mode(&s.0.join("b")), 0o644);
        perform(None, m, false).unwrap();
        assert_eq!(mode(&s.0.join("b")), 0o600);
        // A new folder made again.
        let f = Action::Create { uri: uri(&s.0.join("New Folder")), folder: true, template: None, named: false };
        perform(None, f, false).unwrap();
        assert!(s.0.join("New Folder").is_dir());
        // A rename onto a name in use is refused, nothing changed.
        let clash = Action::Rename { items: vec![(uri(&s.0.join("x")), uri(&s.0.join("c")))] };
        assert!(perform(None, clash, false).is_err());
        assert_eq!(read(&s.0.join("c")), "2");
    }

    fn undo_len() -> usize {
        UNDO.with(|u| u.borrow().len())
    }
    fn redo_len() -> usize {
        REDO.with(|r| r.borrow().len())
    }

    #[test]
    fn undo_list_bookkeeping() {
        // (Each test runs on a thread of its own, with lists of its own.)
        let s = home_scratch("undolist");
        assert!(undo(None).is_err() && redo(None).is_err());
        assert_eq!((undo_label(), redo_label()), (None, None));
        // A new folder named right away is one step, under its new name.
        let made = gio::File::for_path(s.0.join("New Folder"));
        let named = gio::File::for_path(s.0.join("Photos"));
        note_created(&made, true, None);
        note_rename(&made, &named);
        assert_eq!(undo_len(), 1);
        assert_eq!(undo_label().as_deref(), Some("_Undo Creation of Folder “Photos”"));
        // Renamed again later: a step of its own (Undo gives the name back,
        // it doesn't take the folder away).
        let renamed = gio::File::for_path(s.0.join("Pictures"));
        note_rename(&named, &renamed);
        assert_eq!(undo_len(), 2);
        assert_eq!(undo_label().as_deref(), Some("_Undo Rename of “Pictures”"));
        UNDO.with(|u| u.borrow_mut().pop());
        // Renamed to its own name: nothing. Another item: a step of its own.
        note_rename(&named, &named);
        assert_eq!(undo_len(), 1);
        let other = gio::File::for_path(s.0.join("other"));
        note_rename(&made, &other);
        assert_eq!(undo_len(), 2);
        assert_eq!(undo_label().as_deref(), Some("_Undo Rename of “other”"));
        // Nothing changed, nothing noted.
        note_renames(vec![]);
        note_modes(vec![(other.clone(), 0o644, 0o644)]);
        note_chowns(vec![(other.clone(), (1, 1), (1, 1))]);
        assert_eq!(undo_len(), 2);
        // Only what changed: one item of two.
        note_modes(vec![(other, 0o644, 0o600), (named, 0o644, 0o644)]);
        assert_eq!(UNDO.with(|u| u.borrow().last().map(|a| matches!(a, Action::Mode { items } if items.len() == 1))), Some(true));
        // At most UNDO_STEPS, the oldest forgotten.
        for i in 0..UNDO_STEPS + 5 {
            note_renames(vec![(format!("file:///x/{i}"), format!("file:///x/{i}-b"))]);
        }
        assert_eq!(undo_len(), UNDO_STEPS);
        assert_eq!(undo_label().as_deref(), Some(format!("_Undo Rename of “{}-b”", UNDO_STEPS + 4).as_str()));
    }

    #[test]
    fn undo_then_redo_moves_between_the_lists() {
        let s = home_scratch("undoredo-lists");
        write(&s.0.join("a"), "1");
        let (a, b) = (gio::File::for_path(s.0.join("a")), gio::File::for_path(s.0.join("b")));
        fs::rename(s.0.join("a"), s.0.join("b")).unwrap();
        note_rename(&a, &b);
        undo(None).unwrap();
        assert_eq!(read(&s.0.join("a")), "1");
        assert_eq!((undo_len(), redo_len()), (0, 1));
        assert_eq!(redo_label().as_deref(), Some("_Redo Rename of “b”"));
        redo(None).unwrap();
        assert_eq!(read(&s.0.join("b")), "1");
        assert_eq!((undo_len(), redo_len()), (1, 0));
        // Undone again, then something new: nothing to redo any more.
        undo(None).unwrap();
        assert_eq!(redo_len(), 1);
        note_renames(vec![(uri(&s.0.join("a")), uri(&s.0.join("c")))]);
        assert_eq!((undo_len(), redo_len()), (1, 0));
        // A step that can't be undone any more ("c" isn't there): an
        // error, and the step is gone.
        assert!(undo(None).is_err());
        assert_eq!((undo_len(), redo_len()), (0, 0));
    }

    #[test]
    fn undo_and_redo_of_a_group_change() {
        use std::os::unix::fs::MetadataExt;
        let s = home_scratch("undochown");
        write(&s.0.join("f"), "x");
        let p = s.0.join("f");
        let (uid, gid) = (fs::metadata(&p).unwrap().uid(), fs::metadata(&p).unwrap().gid());
        // Another group of ours, if we have one (any user may give a file
        // of theirs to one of their groups).
        // SAFETY: the buffer is as long as getgroups is told.
        let groups = unsafe {
            let n = libc::getgroups(0, std::ptr::null_mut());
            let mut v = vec![0 as libc::gid_t; n.max(0) as usize];
            let n = libc::getgroups(v.len() as i32, v.as_mut_ptr());
            v.truncate(n.max(0) as usize);
            v
        };
        let Some(&other) = groups.iter().find(|&&g| g != gid) else {
            eprintln!("only one group: not tested");
            return;
        };
        std::os::unix::fs::chown(&p, None, Some(other)).unwrap();
        note_chowns(vec![(gio::File::for_path(&p), (uid, gid), (uid, other))]);
        assert_eq!(undo_label().as_deref(), Some("_Undo Change of Owner or Group of “f”"));
        undo(None).unwrap();
        assert_eq!(fs::metadata(&p).unwrap().gid(), gid);
        redo(None).unwrap();
        assert_eq!(fs::metadata(&p).unwrap().gid(), other);
        // Only local files.
        let remote = Action::Chown { items: vec![("sftp://host/f".into(), (uid, gid), (uid, other))] };
        assert!(perform(None, remote, true).is_err());
    }

    fn home_scratch(name: &str) -> TestDir {
        TestDir::new(name)
    }
    /// On another file system than target/, for moves across them.
    fn scratch(base: &str, name: &str) -> TestDir {
        TestDir::in_dir(Path::new(base), name)
    }
    fn write(p: &Path, s: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }
    fn read(p: &Path) -> String {
        fs::read_to_string(p).unwrap()
    }
    fn uri(p: &Path) -> String {
        gio::File::for_path(p).uri().to_string()
    }
    fn transfer(kind: Kind, src: &Path, dest_dir: &Path) -> Job {
        Job::Transfer {
            kind,
            items: vec![Item { src: uri(src), dest_dir: uri(dest_dir), name: None }],
            check: false,
        }
    }

    /// src/d/{a,b,sub/c} merged into dst/d/{a,x}: a is replaced, the rest moves in.
    fn merge_setup(root: &Path, dst_root: &Path) -> (PathBuf, PathBuf) {
        let src = root.join("src/d");
        write(&src.join("a"), "new a");
        write(&src.join("b"), "b");
        write(&src.join("sub/c"), "c");
        let dst = dst_root.join("dst");
        write(&dst.join("d/a"), "old a");
        write(&dst.join("d/x"), "x");
        (src, dst)
    }

    fn check_merged(dst: &Path) {
        assert_eq!(read(&dst.join("d/a")), "new a");
        assert_eq!(read(&dst.join("d/b")), "b");
        assert_eq!(read(&dst.join("d/sub/c")), "c");
        assert_eq!(read(&dst.join("d/x")), "x");
    }

    #[test]
    fn move_merge_same_fs() {
        let s = home_scratch("merge");
        let (src, dst) = merge_setup(&s.0, &s.0);
        let (created, asked) = run_job(transfer(Kind::Move, &src, &dst), &[Resolution::Replace, Resolution::Replace]);
        assert_eq!(asked, ["conflict:d", "conflict:a"]);
        check_merged(&dst);
        assert!(!src.exists(), "source folder should be gone after a full merge");
        assert_eq!(created, [uri(&dst.join("d"))]);
    }

    #[test]
    fn move_merge_keeps_skipped() {
        let s = home_scratch("mergeskip");
        let (src, dst) = merge_setup(&s.0, &s.0);
        let (_, asked) = run_job(transfer(Kind::Move, &src, &dst), &[Resolution::Replace, Resolution::Skip]);
        assert_eq!(asked, ["conflict:d", "conflict:a"]);
        assert_eq!(read(&dst.join("d/a")), "old a");
        assert_eq!(read(&dst.join("d/b")), "b");
        assert_eq!(read(&src.join("a")), "new a", "skipped file stays in the source");
        assert!(!src.join("b").exists());
    }

    #[test]
    fn move_merge_cross_fs() {
        let s = home_scratch("xmerge");
        let t = scratch("/dev/shm", "xmerge");
        let (src, dst) = merge_setup(&t.0, &s.0);
        let (_, asked) = run_job(transfer(Kind::Move, &src, &dst), &[Resolution::Replace, Resolution::Replace]);
        assert_eq!(asked, ["conflict:d", "conflict:a"]);
        check_merged(&dst);
        assert!(!src.exists());
    }

    #[test]
    fn move_cross_fs_plain() {
        let s = home_scratch("xmove");
        let t = scratch("/dev/shm", "xmove");
        let src = t.0.join("d");
        write(&src.join("sub/c"), "c");
        let (created, asked) = run_job(transfer(Kind::Move, &src, &s.0), &[]);
        assert!(asked.is_empty(), "{asked:?}");
        assert_eq!(read(&s.0.join("d/sub/c")), "c");
        assert!(!src.exists());
        assert_eq!(created, [uri(&s.0.join("d"))]);
    }

    #[test]
    fn move_cross_fs_counts_all_items_in_bytes() {
        // One item moves by renaming, two have to be copied: one byte total
        // for those two, not one for each.
        let s = home_scratch("xmovebytes");
        let t = scratch("/dev/shm", "xmovebytes");
        write(&s.0.join("here/a"), &"a".repeat(1000));
        write(&t.0.join("b"), &"b".repeat(3000));
        write(&t.0.join("c/inner"), &"c".repeat(5000));
        let dst = s.0.join("dst");
        fs::create_dir_all(&dst).unwrap();
        let items = [s.0.join("here/a"), t.0.join("b"), t.0.join("c")].iter().map(|p| Item { src: uri(p), dest_dir: uri(&dst), name: None }).collect();
        let (_, asked, _, progress) = run_job_watched(Job::Transfer { kind: Kind::Move, items, check: false }, vec![], Arc::new(AtomicBool::new(false)), gio::Cancellable::new());
        assert!(asked.is_empty(), "{asked:?}");
        assert_eq!(read(&dst.join("c/inner")).len(), 5000);
        assert!(!t.0.join("b").exists() && !t.0.join("c").exists());
        let in_bytes: Vec<&(f64, String)> = progress.iter().filter(|(_, d)| !d.contains("items")).collect();
        assert!(!in_bytes.is_empty(), "{progress:?}");
        let total = format!(" of {}", util::format_size(8000));
        assert!(in_bytes.iter().all(|(_, d)| d.contains(&total) && d.starts_with("Moving")), "{progress:?}");
        assert!(progress.windows(2).all(|w| w[1].0 >= w[0].0), "{progress:?}");
    }

    #[test]
    fn move_folder_over_file_cross_fs() {
        let s = home_scratch("xdiroverfile");
        let t = scratch("/dev/shm", "xdiroverfile");
        write(&t.0.join("n/inner"), "i");
        write(&s.0.join("n"), "file");
        let (_, asked) = run_job(transfer(Kind::Move, &t.0.join("n"), &s.0), &[Resolution::Replace]);
        assert_eq!(asked, ["conflict:n"]);
        assert_eq!(read(&s.0.join("n/inner")), "i");
        assert!(!t.0.join("n").exists());
    }

    #[test]
    fn move_file_over_folder() {
        let s = home_scratch("fileoverdir");
        write(&s.0.join("src/n"), "file");
        write(&s.0.join("dst/n/inner"), "i");
        let (_, asked) = run_job(transfer(Kind::Move, &s.0.join("src/n"), &s.0.join("dst")), &[Resolution::Replace]);
        assert_eq!(asked, ["conflict:n"]);
        assert_eq!(read(&s.0.join("dst/n")), "file");
        assert!(!s.0.join("src/n").exists());
    }

    #[test]
    fn move_folder_over_file() {
        let s = home_scratch("diroverfile");
        write(&s.0.join("src/n/inner"), "i");
        write(&s.0.join("dst/n"), "file");
        let (_, asked) = run_job(transfer(Kind::Move, &s.0.join("src/n"), &s.0.join("dst")), &[Resolution::Replace]);
        assert_eq!(asked, ["conflict:n"]);
        assert_eq!(read(&s.0.join("dst/n/inner")), "i");
        assert!(!s.0.join("src/n").exists());
    }

    #[test]
    fn move_keep_both() {
        let s = home_scratch("keepboth");
        write(&s.0.join("src/a.txt"), "new");
        write(&s.0.join("dst/a.txt"), "old");
        let (created, _) = run_job(transfer(Kind::Move, &s.0.join("src/a.txt"), &s.0.join("dst")), &[Resolution::KeepBoth]);
        assert_eq!(read(&s.0.join("dst/a.txt")), "old");
        assert_eq!(read(&s.0.join("dst/a (2).txt")), "new");
        assert_eq!(created, [uri(&s.0.join("dst/a (2).txt"))]);
    }

    #[test]
    fn copy_merge() {
        let s = home_scratch("copymerge");
        let (src, dst) = merge_setup(&s.0, &s.0);
        let (_, asked) = run_job(transfer(Kind::Copy, &src, &dst), &[Resolution::Replace, Resolution::Replace]);
        assert_eq!(asked, ["conflict:d", "conflict:a"]);
        check_merged(&dst);
        assert_eq!(read(&src.join("a")), "new a", "copy leaves the source alone");
    }

    #[test]
    fn pause_holds_the_job_until_resumed() {
        let s = home_scratch("pause");
        write(&s.0.join("src/a.txt"), "a");
        write(&s.0.join("src/b.txt"), "b");
        fs::create_dir_all(s.0.join("dst")).unwrap();
        let paused = Arc::new(AtomicBool::new(true));
        let job = Job::Transfer {
            kind: Kind::Copy,
            items: ["a.txt", "b.txt"].iter().map(|n| Item { src: uri(&s.0.join("src").join(n)), dest_dir: uri(&s.0.join("dst")), name: None }).collect(),
            check: false,
        };
        let p = paused.clone();
        let worker = std::thread::spawn(move || run_job_paused(job, &[], p));
        std::thread::sleep(Duration::from_millis(400));
        assert!(!worker.is_finished(), "a paused job must not finish");
        assert!(!s.0.join("dst/b.txt").exists(), "nothing past the pause point may be copied");
        paused.store(false, Ordering::Relaxed);
        let (_, asked) = worker.join().unwrap();
        assert!(asked.is_empty(), "{asked:?}");
        assert_eq!(read(&s.0.join("dst/a.txt")), "a");
        assert_eq!(read(&s.0.join("dst/b.txt")), "b");
    }

    #[test]
    fn move_into_itself_refused() {
        let s = home_scratch("intoself");
        write(&s.0.join("d/f"), "f");
        let (_, asked) = run_job(transfer(Kind::Move, &s.0.join("d"), &s.0.join("d")), &[]);
        assert_eq!(asked.len(), 1);
        assert!(asked[0].starts_with("error:Cannot move"), "{asked:?}");
        assert_eq!(read(&s.0.join("d/f")), "f");
    }

    #[test]
    fn delete_leaves_what_links_point_to() {
        let s = home_scratch("deletelinks");
        write(&s.0.join("d/sub/f"), "f");
        write(&s.0.join("outside/keep.txt"), "keep");
        std::os::unix::fs::symlink(s.0.join("outside"), s.0.join("d/sub/dir-link")).unwrap();
        std::os::unix::fs::symlink(s.0.join("outside/keep.txt"), s.0.join("d/file-link")).unwrap();
        let (_, asked) = run_job(Job::Delete(vec![uri(&s.0.join("d"))]), &[]);
        assert!(asked.is_empty(), "{asked:?}");
        assert!(!s.0.join("d").exists());
        assert_eq!(read(&s.0.join("outside/keep.txt")), "keep");
    }

    #[test]
    fn items_inside_are_removed_as_gio_would() {
        use std::os::unix::fs::PermissionsExt;
        let s = home_scratch("removepath");
        write(&s.0.join("full/f"), "f");
        let not_empty = remove_path(&s.0.join("full")).unwrap_err();
        assert!(not_empty.matches(gio::IOErrorEnum::NotEmpty), "{not_empty}");
        assert!(remove_path(&s.0.join("missing")).unwrap_err().matches(gio::IOErrorEnum::NotFound));
        // A link to a folder goes, not the folder.
        std::os::unix::fs::symlink(s.0.join("full"), s.0.join("link")).unwrap();
        remove_path(&s.0.join("link")).unwrap();
        assert_eq!(read(&s.0.join("full/f")), "f");
        remove_path(&s.0.join("full/f")).unwrap();
        remove_path(&s.0.join("full")).unwrap();
        assert!(!s.0.join("full").exists());
        if unsafe { libc::geteuid() } != 0 {
            write(&s.0.join("ro/f"), "f");
            fs::set_permissions(s.0.join("ro"), fs::Permissions::from_mode(0o555)).unwrap();
            let denied = remove_path(&s.0.join("ro/f")).unwrap_err();
            fs::set_permissions(s.0.join("ro"), fs::Permissions::from_mode(0o755)).unwrap();
            assert!(denied.matches(gio::IOErrorEnum::PermissionDenied), "{denied}");
            assert_eq!(denied.message(), format!("Error removing file {}: Permission denied", s.0.join("ro/f").display()));
        }
    }

    #[test]
    fn what_cant_be_deleted_inside_a_folder_is_asked_about_and_kept() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let s = home_scratch("deletestuck");
        write(&s.0.join("d/gone/a"), "a");
        write(&s.0.join("d/ro/kept"), "kept");
        write(&s.0.join("d/b"), "b");
        fs::set_permissions(s.0.join("d/ro"), fs::Permissions::from_mode(0o555)).unwrap();
        let (_, asked) = run_job(Job::Delete(vec![uri(&s.0.join("d"))]), &[]);
        fs::set_permissions(s.0.join("d/ro"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(asked, ["error:Error while deleting “kept”"]);
        assert_eq!(read(&s.0.join("d/ro/kept")), "kept");
        assert!(!s.0.join("d/gone").exists() && !s.0.join("d/b").exists());
    }

    #[test]
    fn copies_into_the_same_folder_get_new_names() {
        let s = home_scratch("samefolder");
        write(&s.0.join("a.txt"), "a");
        write(&s.0.join("d/f"), "f");
        for _ in 0..2 {
            let (_, asked) = run_job(transfer(Kind::Copy, &s.0.join("a.txt"), &s.0), &[]);
            assert!(asked.is_empty(), "{asked:?}");
        }
        let (created, _) = run_job(transfer(Kind::Copy, &s.0.join("d"), &s.0), &[]);
        assert_eq!(read(&s.0.join("a (copy).txt")), "a");
        assert_eq!(read(&s.0.join("a (copy 2).txt")), "a");
        assert_eq!(read(&s.0.join("d (copy)/f")), "f");
        assert_eq!(created, [uri(&s.0.join("d (copy)"))]);
        // Moving something to where it is changes nothing.
        let (_, asked) = run_job(transfer(Kind::Move, &s.0.join("a.txt"), &s.0), &[]);
        assert!(asked.is_empty(), "{asked:?}");
        assert_eq!(read(&s.0.join("a.txt")), "a");
    }

    #[test]
    fn copied_links_stay_links() {
        let s = home_scratch("copylinks");
        write(&s.0.join("src/target.txt"), "t");
        std::os::unix::fs::symlink("target.txt", s.0.join("src/link")).unwrap();
        std::os::unix::fs::symlink("/no/such/place", s.0.join("src/dangling")).unwrap();
        fs::create_dir_all(s.0.join("dst")).unwrap();
        let (_, asked) = run_job(transfer(Kind::Copy, &s.0.join("src"), &s.0.join("dst")), &[]);
        assert!(asked.is_empty(), "{asked:?}");
        assert_eq!(fs::read_link(s.0.join("dst/src/link")).unwrap(), Path::new("target.txt"));
        assert_eq!(fs::read_link(s.0.join("dst/src/dangling")).unwrap(), Path::new("/no/such/place"));
        assert_eq!(read(&s.0.join("dst/src/link")), "t");
    }

    #[test]
    fn unreadable_files_are_skipped_without_a_partial_copy() {
        use std::os::unix::fs::PermissionsExt;
        // Root reads everything: nothing to test then.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let s = home_scratch("unreadable");
        write(&s.0.join("src/d/ok"), "ok");
        write(&s.0.join("src/d/secret"), "secret");
        fs::set_permissions(s.0.join("src/d/secret"), fs::Permissions::from_mode(0o000)).unwrap();
        fs::create_dir_all(s.0.join("dst")).unwrap();
        let (_, asked) = run_job(transfer(Kind::Copy, &s.0.join("src/d"), &s.0.join("dst")), &[]);
        assert_eq!(asked, ["error:Error while copying “secret”"]);
        assert_eq!(read(&s.0.join("dst/d/ok")), "ok");
        assert!(!s.0.join("dst/d/secret").exists(), "a skipped file leaves nothing behind");
        fs::set_permissions(s.0.join("src/d/secret"), fs::Permissions::from_mode(0o644)).unwrap();
    }

    #[test]
    fn links_point_to_the_original() {
        let s = home_scratch("link");
        write(&s.0.join("a.txt"), "a");
        fs::create_dir_all(s.0.join("dst")).unwrap();
        run_job(transfer(Kind::Link, &s.0.join("a.txt"), &s.0.join("dst")), &[]);
        assert_eq!(fs::read_link(s.0.join("dst/a.txt")).unwrap(), s.0.join("a.txt"));
        // In the same folder, as Caja names them.
        let (created, _) = run_job(transfer(Kind::Link, &s.0.join("a.txt"), &s.0), &[]);
        assert_eq!(fs::read_link(s.0.join("Link to a.txt")).unwrap(), s.0.join("a.txt"));
        assert_eq!(created, [uri(&s.0.join("Link to a.txt"))]);
    }

    #[test]
    fn keep_both_for_a_folder() {
        let s = home_scratch("keepbothdir");
        write(&s.0.join("src/d/new"), "new");
        write(&s.0.join("dst/d/old"), "old");
        let (created, asked) = run_job(transfer(Kind::Copy, &s.0.join("src/d"), &s.0.join("dst")), &[Resolution::KeepBoth]);
        assert_eq!(asked, ["conflict:d"]);
        assert_eq!(read(&s.0.join("dst/d/old")), "old");
        assert!(!s.0.join("dst/d/new").exists());
        assert_eq!(read(&s.0.join("dst/d (2)/new")), "new");
        assert_eq!(created, [uri(&s.0.join("dst/d (2)"))]);
    }

    #[test]
    fn cancel_stops_the_remaining_items() {
        let s = home_scratch("cancel");
        for n in ["a", "b"] {
            write(&s.0.join("src").join(n), "new");
        }
        write(&s.0.join("dst/a"), "old");
        let job = Job::Transfer {
            kind: Kind::Move,
            items: ["a", "b"].iter().map(|n| Item { src: uri(&s.0.join("src").join(n)), dest_dir: uri(&s.0.join("dst")), name: None }).collect(),
            check: false,
        };
        let (_, asked) = run_job(job, &[Resolution::Cancel]);
        assert_eq!(asked, ["conflict:a"]);
        assert_eq!(read(&s.0.join("dst/a")), "old");
        assert!(!s.0.join("dst/b").exists());
        assert_eq!(read(&s.0.join("src/b")), "new");
    }

    /// A tree of `files` files (sizes up to 64 KiB, some empty) in
    /// folders up to four deep, with links to files and folders in it.
    fn make_tree(root: &Path, files: usize, seed: u64) {
        let mut r = crate::testutil::Rng::new(seed);
        fs::create_dir_all(root).unwrap();
        let mut dirs = vec![root.to_path_buf()];
        for i in 0..files {
            if r.chance(12) && dirs.len() < files / 10 + 2 {
                let parent = r.pick(&dirs).clone();
                if parent.components().count() - root.components().count() < 4 {
                    let d = parent.join(format!("dir {i}"));
                    fs::create_dir_all(&d).unwrap();
                    dirs.push(d);
                }
            }
            let d = r.pick(&dirs).clone();
            let size = if r.chance(10) { 0 } else { r.below(64 * 1024) };
            let data: Vec<u8> = (0..size).map(|j| ((i * 31 + j) % 251) as u8).collect();
            fs::write(d.join(format!("file {i}.bin")), data).unwrap();
        }
        std::os::unix::fs::symlink("file 0.bin", root.join("link to a file")).unwrap();
        std::os::unix::fs::symlink(".", root.join("link to here")).unwrap();
    }

    /// Every entry under `root`, relative, with its content (a link's target).
    fn tree_contents(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        let mut out = std::collections::BTreeMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                let rel = p.strip_prefix(root).unwrap().to_path_buf();
                let meta = fs::symlink_metadata(&p).unwrap();
                if meta.file_type().is_symlink() {
                    out.insert(rel, fs::read_link(&p).unwrap().into_os_string().into_encoded_bytes());
                } else if meta.is_dir() {
                    out.insert(rel, b"<dir>".to_vec());
                    stack.push(p);
                } else {
                    out.insert(rel, fs::read(&p).unwrap());
                }
            }
        }
        out
    }

    fn files_count() -> usize {
        std::env::var("ARKHA_STRESS_FILES").ok().and_then(|v| v.parse().ok()).unwrap_or(3000)
    }

    /// cargo test -- --ignored (ARKHA_STRESS_FILES=n for another size).
    #[test]
    #[ignore = "stress test: run with --ignored"]
    fn stress_big_tree_copy_move_delete() {
        let s = home_scratch("stress-tree");
        let src = s.0.join("src/tree");
        make_tree(&src, files_count(), 1);
        let want = tree_contents(&src);
        // Copy, here.
        fs::create_dir_all(s.0.join("copy")).unwrap();
        let (_, asked) = run_job(transfer(Kind::Copy, &src, &s.0.join("copy")), &[]);
        assert!(asked.is_empty(), "{asked:?}");
        assert!(tree_contents(&s.0.join("copy/tree")) == want, "the copy differs");
        // Move to another file system (RAM: a small tree only) and back.
        let shm = scratch("/dev/shm", "stress-tree");
        let small = s.0.join("small/tree");
        make_tree(&small, 300, 2);
        let small_want = tree_contents(&small);
        run_job(transfer(Kind::Move, &small, &shm.0), &[]);
        assert!(!small.exists() && tree_contents(&shm.0.join("tree")) == small_want, "moving away");
        run_job(transfer(Kind::Move, &shm.0.join("tree"), &s.0.join("small")), &[]);
        assert!(!shm.0.join("tree").exists() && tree_contents(&small) == small_want, "moving back");
        // Move here (a rename), then delete it all.
        run_job(transfer(Kind::Move, &s.0.join("copy/tree"), &s.0), &[]);
        assert!(tree_contents(&s.0.join("tree")) == want);
        let (_, asked) = run_job(Job::Delete(vec![uri(&s.0.join("tree")), uri(&src)]), &[]);
        assert!(asked.is_empty(), "{asked:?}");
        assert!(!s.0.join("tree").exists() && !src.exists());
    }

    /// Copies and moves cancelled at random moments: what got copied is
    /// whole, nothing is lost, and nothing half-written is left behind.
    #[test]
    #[ignore = "stress test: run with --ignored"]
    fn stress_cancel_anywhere() {
        let s = home_scratch("stress-cancel");
        let src = s.0.join("src/tree");
        make_tree(&src, files_count() / 3, 3);
        let want = tree_contents(&src);
        let mut r = crate::testutil::Rng::new(4);
        for round in 0..12 {
            let dst = s.0.join(format!("dst{round}"));
            fs::create_dir_all(&dst).unwrap();
            let moving = round % 3 == 2;
            // Moving within a file system is a rename: across one it copies.
            let shm = scratch("/dev/shm", &format!("stress-cancel-{round}"));
            let (from, to) = if moving {
                let from = shm.0.join("tree");
                make_tree(&from, 400, 10 + round as u64);
                (from, dst)
            } else {
                (src.clone(), dst)
            };
            let before = tree_contents(&from);
            let cancel = gio::Cancellable::new();
            let c = cancel.clone();
            let delay = r.below(400) as u64;
            let canceller = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(delay));
                c.cancel();
            });
            let job = transfer(if moving { Kind::Move } else { Kind::Copy }, &from, &to);
            let (_, asked) = run_job_with(job, &[], Arc::new(AtomicBool::new(false)), cancel);
            canceller.join().unwrap();
            assert!(asked.is_empty(), "round {round}: {asked:?}");
            let got = if to.join("tree").exists() { tree_contents(&to.join("tree")) } else { Default::default() };
            let left = if from.exists() { tree_contents(&from) } else { Default::default() };
            for (rel, data) in &got {
                // A folder may be there half full; a file only whole.
                let orig = before.get(rel).unwrap_or_else(|| panic!("round {round}: {rel:?} came from nowhere"));
                assert!(data == orig || data == b"<dir>", "round {round}: {rel:?} is cut off ({} of {} bytes)", data.len(), orig.len());
            }
            for (rel, data) in &before {
                if data != b"<dir>" {
                    assert!(got.contains_key(rel) || left.get(rel) == Some(data), "round {round}: {rel:?} was lost");
                }
            }
            if !moving {
                assert!(tree_contents(&src) == want, "round {round}: the source changed");
            }
        }
    }

    #[test]
    fn time_left_as_caja_words_it() {
        assert_eq!(format_time_left(1), "1 second");
        assert_eq!(format_time_left(59), "59 seconds");
        assert_eq!(format_time_left(60), "1 minute");
        assert_eq!(format_time_left(3599), "59 minutes");
        assert_eq!(format_time_left(3600 + 60), "1 hour, 1 minute");
        assert_eq!(format_time_left(3 * 3600 + 20 * 60), "3 hours, 20 minutes");
        assert_eq!(format_time_left(5 * 3600 + 59), "approximately 5 hours");
    }

    #[test]
    fn time_left_once_the_rate_is_steady() {
        let (s, f) = (Duration::from_secs, util::format_size);
        // Not before 15 seconds.
        assert_eq!(bytes_detail(1_000_000, 4_000_000, s(14)), format!("{} of {}", f(1_000_000), f(4_000_000)));
        // 1 MB in 20 s: the other 3 MB take a minute.
        assert_eq!(bytes_detail(1_000_000, 4_000_000, s(20)), format!("{} of {} \u{2014} 1 minute left ({}/sec)", f(1_000_000), f(4_000_000), f(50_000)));
        assert_eq!(items_detail(10, 40, s(30)), "10 of 40 items \u{2014} 1 minute left");
        assert_eq!(items_detail(0, 40, s(30)), "0 of 40 items");
    }

    #[test]
    fn copy_in_pieces_like_gio() {
        let s = home_scratch("pieces");
        let src = s.0.join("src.bin");
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        fs::write(&src, &data).unwrap();
        let none = gio::Cancellable::new();
        let mut seen = vec![];
        let dst = s.0.join("dst.bin");
        assert!(copy_in_pieces(&src, &dst, false, &none, &mut |done, total| seen.push((done, total))).unwrap().is_ok());
        assert_eq!(fs::read(&dst).unwrap(), data);
        assert_eq!(seen.last(), Some(&(300_000, 300_000)));
        // An existing file isn't replaced unless asked.
        let r = copy_in_pieces(&src, &dst, false, &none, &mut |_, _| {}).unwrap();
        assert!(r.unwrap_err().matches(gio::IOErrorEnum::Exists));
        // Replacing a longer file leaves just the copy.
        fs::write(&dst, vec![7u8; 500_000]).unwrap();
        assert!(copy_in_pieces(&src, &dst, true, &none, &mut |_, _| {}).unwrap().is_ok());
        assert_eq!(fs::read(&dst).unwrap(), data);
        // A cancelled copy leaves nothing behind.
        let cancelled = gio::Cancellable::new();
        cancelled.cancel();
        let gone = s.0.join("gone.bin");
        let r = copy_in_pieces(&src, &gone, false, &cancelled, &mut |_, _| {}).unwrap();
        assert!(r.unwrap_err().matches(gio::IOErrorEnum::Cancelled));
        assert!(!gone.exists());
    }
}
