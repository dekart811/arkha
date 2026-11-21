//! Files dragged out of an archive manager (Engrampa, Xarchiver, File
//! Roller before 44; File Roller 44 and later can't drag files out; KDE's
//! Ark has a way of its own, see arkdnd.rs). They aren't files yet, so the
//! archive manager offers the X Direct Save protocol (XDS) instead of a
//! list of files: the drop target writes where they should go into a
//! property of the source's window and asks for the data, and the source
//! then extracts them there itself and answers "S" (saved), "E" (error) or
//! "F" (take the data instead, which archive managers don't offer). X11
//! only: the property is an X window property (on Wayland, both programs
//! can run through XWayland).

use gtk::prelude::*;

pub const TARGET: &str = "XdndDirectSave0";

fn atom() -> gdk::Atom {
    gdk::Atom::intern(TARGET)
}

fn text() -> gdk::Atom {
    gdk::Atom::intern("text/plain")
}

/// Whether XDS can be used here (on X11).
fn available() -> bool {
    gdk::Display::default().is_some_and(|d| d.type_().name().contains("X11"))
}

/// The drop target entry for XDS, where it can be used.
pub fn target_entry() -> Option<gtk::TargetEntry> {
    available().then(|| gtk::TargetEntry::new(TARGET, gtk::TargetFlags::OTHER_APP, 0))
}

/// Whether `target` (as drag_dest_find_target chose it) is XDS.
pub fn is(target: &gdk::Atom) -> bool {
    target.name() == TARGET
}

/// What the source suggests calling the file (a name, never a path).
fn suggested_name(win: &gdk::Window) -> Option<String> {
    let (_, format, data) = gdk::property_get(win, &atom(), &text(), 0, 4096, 0)?;
    if format != 8 {
        return None;
    }
    name_from(&data)
}

/// The file name in what a source suggests: its last part, if it's a
/// name, without leading dots: dragged in, a file isn't made hidden (a
/// ".bash_profile" in the home folder would run at the next login).
fn name_from(data: &[u8]) -> Option<String> {
    let raw = String::from_utf8_lossy(data).trim_end_matches('\0').to_string();
    let name = raw.rsplit('/').next().unwrap_or("").trim_start_matches('.').to_string();
    (!name.is_empty()).then_some(name)
}

/// On the drop: tells the source to save into folder `dest` and asks for
/// its answer (handled by `finish` in drag-data-received). False if that
/// can't be done: then finish the drag as failed.
pub fn start(widget: &impl IsA<gtk::Widget>, ctx: &gdk::DragContext, dest: &gio::File, time: u32) -> bool {
    if !available() {
        return false;
    }
    let win = ctx.source_window();
    let Some(name) = suggested_name(&win) else { return false };
    // A name that's taken gets a free one: the source writes where it's
    // told, and an existing file there would be replaced.
    let uri = dest.child(crate::util::unique_name(dest, &name, false)).uri();
    gdk::property_change(&win, &atom(), &text(), 8, gdk::PropMode::Replace, gdk::ChangeData::UChars(uri.as_bytes()));
    widget.as_ref().drag_get_data(ctx, &atom(), time);
    true
}

/// The source's answer, in drag-data-received: finishes the drag. Whether
/// it saved the files (the folder's monitor then shows them).
pub fn finish(ctx: &gdk::DragContext, sel: &gtk::SelectionData, time: u32) -> bool {
    let saved = sel.data().first() == Some(&b'S');
    ctx.drag_finish(saved, false, time);
    gdk::property_delete(&ctx.source_window(), &atom());
    saved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_names_stay_in_the_folder() {
        assert_eq!(name_from(b"report.pdf\0").as_deref(), Some("report.pdf"));
        assert_eq!(name_from(b"../../.bashrc").as_deref(), Some("bashrc"));
        assert_eq!(name_from(b"...hidden.txt").as_deref(), Some("hidden.txt"));
        assert_eq!(name_from(b"/etc/passwd").as_deref(), Some("passwd"));
        for bad in [&b""[..], b"..", b".", b"a/", b"/", b"x/.."] {
            assert_eq!(name_from(bad), None, "{:?}", String::from_utf8_lossy(bad));
        }
    }
}
