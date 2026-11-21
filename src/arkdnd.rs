//! Files dragged out of KDE's Ark. Like the archive managers that use XDS
//! (see xds.rs), Ark has no files to offer yet, but it has a way of its own,
//! which Dolphin uses too and which works on X11 and Wayland alike: the drag
//! carries Ark's D-Bus connection name and an object path (as plain text,
//! nothing else), and the drop target calls that object's
//! org.kde.ark.DndExtract.extractSelectedFilesTo with the folder to put
//! them in. Ark then extracts the items selected in its window there, in
//! the background, and reports its own errors.
//!
//! On a drop, `start` asks for the connection name, `received` then for
//! the path, and once both are there it finishes the drag and calls Ark.

use gtk::prelude::*;

use crate::i18n::{fill, tr};

const SERVICE: &str = "application/x-kde-ark-dndextract-service";
const PATH: &str = "application/x-kde-ark-dndextract-path";
const INTERFACE: &str = "org.kde.ark.DndExtract";
const METHOD: &str = "extractSelectedFilesTo";

/// Kept on the drag between the two requests.
const PENDING: &str = "arkha-ark-extract";
struct Pending {
    /// Where the files go: a path, or a URI for a place that has none.
    dest: String,
    service: Option<String>,
}

/// The drop target entries for Ark's drags, connection name first (which
/// drag_dest_find_target picks), the path after it: GTK only hands a
/// widget data it has a target for.
pub fn target_entries() -> [gtk::TargetEntry; 2] {
    [SERVICE, PATH].map(|t| gtk::TargetEntry::new(t, gtk::TargetFlags::OTHER_APP, 0))
}

/// Whether `target` (as drag_dest_find_target chose it) is a drag from Ark.
pub fn is(target: &gdk::Atom) -> bool {
    target.name() == SERVICE
}

/// The action to show while a drag is over `widget`: copy for one from Ark
/// (it extracts, whatever the keys held: Shift's move or Ctrl+Shift's link
/// mean nothing to it, and are better not passed on), else `other`.
pub fn action(widget: &impl IsA<gtk::Widget>, ctx: &gdk::DragContext, other: gdk::DragAction) -> gdk::DragAction {
    match widget.as_ref().drag_dest_find_target(ctx, None) {
        Some(t) if is(&t) => gdk::DragAction::COPY,
        _ => other,
    }
}

/// On the drop: notes that the files go into `dest` and asks for Ark's
/// connection name (handled by `received` in drag-data-received).
pub fn start(widget: &impl IsA<gtk::Widget>, ctx: &gdk::DragContext, dest: &gio::File, time: u32) {
    let dest = dest.path().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| dest.uri().to_string());
    // SAFETY: only this module reads it, always as a Pending.
    unsafe { ctx.set_data(PENDING, Pending { dest, service: None }) };
    widget.as_ref().drag_get_data(ctx, &gdk::Atom::intern(SERVICE), time);
}

/// In drag-data-received: whether `sel` is part of a drag from Ark (then
/// it's handled here, the drag finished once it's complete).
pub fn received(widget: &impl IsA<gtk::Widget>, ctx: &gdk::DragContext, sel: &gtk::SelectionData, time: u32) -> bool {
    let target = sel.target().name();
    if target != SERVICE && target != PATH {
        return false;
    }
    let text = String::from_utf8_lossy(&sel.data()).trim_end_matches('\0').trim().to_string();
    // SAFETY: as in start.
    let Some(mut pending) = (unsafe { ctx.steal_data::<Pending>(PENDING) }) else {
        ctx.drag_finish(false, false, time);
        return true;
    };
    if target == SERVICE {
        if !gio::dbus_is_name(&text) {
            ctx.drag_finish(false, false, time);
            return true;
        }
        pending.service = Some(text);
        // SAFETY: as in start.
        unsafe { ctx.set_data(PENDING, pending) };
        widget.as_ref().drag_get_data(ctx, &gdk::Atom::intern(PATH), time);
        return true;
    }
    let (Some(service), true) = (pending.service, glib::Variant::is_object_path(&text)) else {
        ctx.drag_finish(false, false, time);
        return true;
    };
    ctx.drag_finish(true, false, time);
    let parent = widget.as_ref().toplevel().and_then(|t| t.downcast::<gtk::Window>().ok());
    extract(&service, &text, &pending.dest, parent);
    true
}

/// Asks Ark (`service`, object `path`) to extract its selection into `dest`;
/// says so if it can't be reached.
fn extract(service: &str, path: &str, dest: &str, parent: Option<gtk::Window>) {
    let failed = move |message: String| {
        crate::util::error_dialog(parent.as_ref(), &tr("Could not extract the files"), &fill(&tr("Ark didn’t answer: {error}"), &[("error", &message)]));
    };
    let bus = match gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        Ok(b) => b,
        Err(e) => return failed(e.message().to_string()),
    };
    bus.call(
        Some(service),
        path,
        INTERFACE,
        METHOD,
        Some(&(dest,).to_variant()),
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        30_000,
        gio::Cancellable::NONE,
        move |result| {
            if let Err(e) = result {
                failed(e.message().to_string());
            }
        },
    );
}
