//! The side pane: Computer, Devices and Bookmarks sections with headings,
//! like Caja's. A section can be put somewhere else by dragging its heading
//! onto another one; bookmarks are reordered the same way.
//!
//! Devices (drives, partitions, phones over MTP, network mounts) are mounted
//! by clicking them, and unmounted / ejected / safely removed with the button
//! next to them or the context menu. Arkha mounts newly inserted media
//! itself (see `init_automount`), unless that's turned off in the
//! preferences, a setting shared with the desktop's own (see
//! `DesktopMount`).

use crate::i18n::{fill, n_, tr, trn};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gio::prelude::*;
use gtk::prelude::*;

use crate::{config, util};

/// Notifications for taking out a drive, like Caja's. GVfs reports how
/// unmounting goes through the mount operation's show-unmount-progress
/// ("summary\nbody" messages): when it takes a while, "Writing data to …",
/// which stays until the data is written; then "… can be safely
/// unplugged", which takes its place.
fn notify_unmount_progress(op: &gtk::MountOperation) {
    #[derive(Default)]
    struct Progress {
        /// The "Writing data" notification's id once it's shown.
        id: Cell<u32>,
        /// Asked for and not shown yet.
        pending: Cell<bool>,
        /// Done (or given up) while it was pending: close it when shown.
        finished: Cell<bool>,
    }
    let progress = Rc::new(Progress::default());
    let close = |p: &Progress| {
        if p.pending.get() {
            p.finished.set(true);
        }
        util::close_notification(p.id.replace(0));
    };
    let p = progress.clone();
    op.connect_show_unmount_progress(move |_, message, _time_left, bytes_left| {
        let (summary, body) = message.split_once('\n').unwrap_or((message, ""));
        if bytes_left == 0 {
            close(&p);
            if !message.is_empty() {
                let n = util::Notification { summary, body, icon: "media-removable", sticky: false };
                util::send_notification(&n, |_| {});
            }
            return;
        }
        // Shown already (GVfs says it once).
        if p.pending.get() || p.id.get() != 0 || p.finished.get() {
            return;
        }
        p.pending.set(true);
        let n = util::Notification { summary, body, icon: "media-removable", sticky: true };
        let p = p.clone();
        util::send_notification(&n, move |id| {
            p.pending.set(false);
            if p.finished.get() {
                util::close_notification(id);
            } else {
                p.id.set(id);
            }
        });
    });
    // Cancelled (files still in use, say): no more "Writing data".
    op.connect_aborted(move |_| close(&progress));
}

/// Section ids (what `sidebar_sections` stores) and headings.
pub const SECTIONS: &[(&str, &str)] = &[("computer", n_("Computer")), ("devices", n_("Devices")), ("bookmarks", n_("Bookmarks"))];

const SECTION_TARGET: &str = "ARKHA_SIDEBAR_SECTION";
const BOOKMARK_TARGET: &str = "ARKHA_SIDEBAR_BOOKMARK";
/// A Computer entry being dragged to another place in its section. Only
/// this (no file list) goes with it, so it can't be dropped on a folder,
/// here or anywhere, and move the folder.
const PLACE_TARGET: &str = "ARKHA_SIDEBAR_PLACE";

#[derive(Clone, Copy, PartialEq)]
pub enum OpenIn {
    Current,
    Tab,
    Window,
}

/// Where the side pane's requests go (the window).
type Handler = Rc<dyn Fn(Request)>;

/// What the side pane asks of its window.
pub enum Request {
    Open(gio::File, OpenIn),
    Drop { uris: Vec<String>, dest: gio::File, action: gdk::DragAction },
    /// A mount went away: leave any folder shown from it.
    Unmounted(gio::File),
    Status(String),
}

#[derive(Clone)]
enum Target {
    Location(gio::File),
    /// Not mounted: clicking mounts it.
    Volume(gio::Volume),
    /// A drive without media (an empty card reader, say): clicking checks.
    Drive(gio::Drive),
}

#[derive(Clone)]
struct Item {
    section: &'static str,
    /// What `hidden_places` stores (devices and bookmarks can't be hidden: None).
    key: Option<String>,
    label: String,
    icon: Option<gio::Icon>,
    tooltip: Option<String>,
    target: Target,
    mount: Option<gio::Mount>,
    volume: Option<gio::Volume>,
    drive: Option<gio::Drive>,
    bookmark: Option<String>,
    trash: bool,
}

enum Row {
    Header(&'static str),
    Item(Item),
}

/// A place that can be hidden in the preferences.
pub struct Place {
    pub key: String,
    pub label: String,
    pub icon: Option<gio::Icon>,
}

pub struct Sidebar {
    pub widget: gtk::ScrolledWindow,
    list: gtk::ListBox,
    rows: RefCell<Vec<Row>>,
    location: RefCell<Option<gio::File>>,
    handler: RefCell<Option<Handler>>,
    monitor: gio::VolumeMonitor,
    signals: RefCell<Vec<glib::SignalHandlerId>>,
    _trash_monitor: RefCell<Option<gio::FileMonitor>>,
    refresh_queued: Cell<bool>,
    /// Where a drop landed, kept from drag-drop until the data arrives.
    drop_y: Cell<i32>,
    /// The item a left button press was on (its signature), opened if the
    /// button is let go on it: by what it is, not where, as the rows may
    /// have been rebuilt meanwhile.
    pressed: RefCell<Option<String>>,
    /// Likewise for headings, which collapse / expand their section.
    pressed_header: Cell<Option<&'static str>>,
}

thread_local! {
    static ALL: RefCell<Vec<Weak<Sidebar>>> = const { RefCell::new(vec![]) };
}

/// Refreshes every window's side pane (after a settings change).
pub fn refresh_all() {
    ALL.with(|a| {
        for s in a.borrow().iter().filter_map(Weak::upgrade) {
            s.refresh();
        }
    });
}

impl Sidebar {
    pub fn new() -> Rc<Self> {
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.style_context().add_class("sidebar");
        list.style_context().add_class("arkha-sidebar");
        let widget = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        widget.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        widget.style_context().add_class("sidebar");
        widget.add(&list);
        widget.set_size_request(140, -1);
        let s = Rc::new(Sidebar {
            widget,
            list,
            rows: RefCell::new(vec![]),
            location: RefCell::new(None),
            handler: RefCell::new(None),
            monitor: gio::VolumeMonitor::get(),
            signals: RefCell::new(vec![]),
            _trash_monitor: RefCell::new(None),
            refresh_queued: Cell::new(false),
            drop_y: Cell::new(0),
            pressed: RefCell::new(None),
            pressed_header: Cell::new(None),
        });
        s.show_background();
        s.connect();
        s.refresh();
        ALL.with(|a| a.borrow_mut().push(Rc::downgrade(&s)));
        s
    }

    /// The background chosen for the side pane (a colour or pattern dropped
    /// on it), or the theme's.
    pub fn show_background(&self) {
        // One setting for every window: on the screen, for Arkha's side
        // panes only (their rows included, rebuilt as they are).
        thread_local! {
            static CSS: gtk::CssProvider = {
                let css = gtk::CssProvider::new();
                if let Some(screen) = gdk::Screen::default() {
                    gtk::StyleContext::add_provider_for_screen(&screen, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
                }
                css
            };
        }
        let (color, image) = config::with(|s| (s.side_pane_color.clone(), s.side_pane_image.clone()));
        let mut css = crate::backgrounds::css("list.arkha-sidebar", &color, &image);
        // The rows show it through, but keep the theme's look when hovered or selected.
        if !css.is_empty() {
            css.push_str("\nlist.arkha-sidebar row:not(:selected):not(:hover) { background-color: transparent; background-image: none; }");
        }
        CSS.with(|p| {
            let _ = p.load_from_data(css.as_bytes());
        });
    }

    pub fn set_handler(&self, f: impl Fn(Request) + 'static) {
        *self.handler.borrow_mut() = Some(Rc::new(f));
    }

    fn request(&self, r: Request) {
        let h = self.handler.borrow().clone();
        if let Some(h) = h {
            h(r);
        }
    }

    fn parent_window(&self) -> Option<gtk::Window> {
        self.widget.toplevel().and_then(|t| t.downcast::<gtk::Window>().ok())
    }

    fn connect(self: &Rc<Self>) {
        let w = Rc::downgrade(self);
        // Device changes rebuild the list (once, however many arrive).
        let queue = {
            let w = w.clone();
            move || {
                if let Some(s) = w.upgrade() {
                    s.queue_refresh();
                }
            }
        };
        let m = &self.monitor;
        let mut ids = vec![];
        macro_rules! on {
            ($connect:ident) => {{
                let q = queue.clone();
                ids.push(m.$connect(move |_, _| q()));
            }};
        }
        on!(connect_volume_added);
        on!(connect_volume_removed);
        on!(connect_volume_changed);
        on!(connect_mount_added);
        on!(connect_mount_changed);
        on!(connect_drive_connected);
        on!(connect_drive_disconnected);
        on!(connect_drive_changed);
        {
            let w = w.clone();
            ids.push(m.connect_mount_removed(move |_, mount| {
                if let Some(s) = w.upgrade() {
                    s.queue_refresh();
                    s.request(Request::Unmounted(mount.root()));
                }
            }));
        }
        *self.signals.borrow_mut() = ids;
        {
            let w = w.clone();
            self.widget.connect_destroy(move |_| {
                if let Some(s) = w.upgrade() {
                    for id in s.signals.borrow_mut().drain(..) {
                        s.monitor.disconnect(id);
                    }
                }
            });
        }
        // The trash icon shows whether it's empty.
        if let Ok(tm) = gio::File::for_uri("trash:///").monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
            tm.connect_changed(move |_, _, _, _| queue());
            *self._trash_monitor.borrow_mut() = Some(tm);
        }

        // Rows open on clicks themselves (see item_row): the list loses
        // track of clicks after a drag. This is for the keyboard (Enter).
        self.list.set_activate_on_single_click(false);
        {
            self.list.connect_row_activated(move |_, row| {
                let mouse = gtk::current_event().is_some_and(|e| {
                    matches!(e.event_type(), gdk::EventType::ButtonPress | gdk::EventType::DoubleButtonPress | gdk::EventType::ButtonRelease)
                });
                if let (Some(s), false) = (w.upgrade(), mouse) {
                    s.activate(row.index(), OpenIn::Current);
                }
            });
        }
        self.setup_drop();
    }

    fn queue_refresh(self: &Rc<Self>) {
        if self.refresh_queued.replace(true) {
            return;
        }
        let w = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(s) = w.upgrade() {
                s.refresh_queued.set(false);
                s.refresh();
            }
        });
    }

    /// Rebuilds the list from the places, devices and bookmarks.
    pub fn refresh(self: &Rc<Self>) {
        let cfg = config::get();
        let mut rows = vec![];
        for id in &cfg.sidebar_sections {
            let Some(&(id, _)) = SECTIONS.iter().find(|(s, _)| s == id) else { continue };
            let items: Vec<Item> = self
                .items(id)
                .into_iter()
                .filter(|i| i.key.as_ref().is_none_or(|k| !cfg.hidden_places.contains(k)))
                .collect();
            // A section without anything to show leaves out its heading too.
            if items.is_empty() {
                continue;
            }
            rows.push(Row::Header(id));
            // A collapsed section shows only its heading.
            if !cfg.sidebar_collapsed.iter().any(|c| c == id) {
                rows.extend(items.into_iter().map(Row::Item));
            }
        }
        // Unchanged (the trash or a device sends many changes, right after
        // starting most of all): the rows stay. Rebuilt under a click, the
        // row it began on would be gone.
        let same = {
            let old = self.rows.borrow();
            old.len() == rows.len() && old.iter().zip(&rows).all(|(a, b)| a.signature() == b.signature())
        };
        if same && !self.list.children().is_empty() {
            *self.rows.borrow_mut() = rows;
            self.select_location();
            return;
        }
        for c in self.list.children() {
            self.list.remove(&c);
        }
        for (i, r) in rows.iter().enumerate() {
            let row = match r {
                Row::Header(id) => self.header_row(id),
                Row::Item(item) => self.item_row(item, i),
            };
            self.list.add(&row);
        }
        *self.rows.borrow_mut() = rows;
        self.list.show_all();
        self.select_location();
    }

    /// Highlights the row of the folder shown in the window.
    pub fn set_location(&self, f: &gio::File) {
        *self.location.borrow_mut() = Some(f.clone());
        self.select_location();
    }

    fn select_location(&self) {
        let loc = self.location.borrow().clone();
        let rows = self.rows.borrow();
        let hit = loc.and_then(|f| {
            rows.iter().position(|r| matches!(r, Row::Item(Item { target: Target::Location(l), .. }) if l.equal(&f)))
        });
        match hit.and_then(|i| self.list.row_at_index(i as i32)) {
            Some(row) => self.list.select_row(Some(&row)),
            None => self.list.unselect_all(),
        }
    }

    /// The Computer entries, for the preferences (devices and bookmarks
    /// can't be hidden).
    pub fn places(self: &Rc<Self>) -> Vec<Place> {
        self.items("computer")
            .into_iter()
            .filter_map(|i| Some(Place { key: i.key?, label: i.label, icon: i.icon }))
            .collect()
    }

    fn items(&self, section: &'static str) -> Vec<Item> {
        match section {
            "computer" => computer_items(),
            "devices" => self.device_items(),
            _ => bookmark_items(),
        }
    }

    fn device_items(&self) -> Vec<Item> {
        let m = &self.monitor;
        let mut out = vec![];
        for d in m.connected_drives() {
            let vols = d.volumes();
            if vols.is_empty() {
                if d.is_media_removable() && !d.is_media_check_automatic() {
                    let name = d.name().to_string();
                    out.push(Item {
                        icon: Some(d.symbolic_icon()),
                        tooltip: Some(name.clone()),
                        label: name,
                        target: Target::Drive(d.clone()),
                        drive: Some(d),
                        ..Item::blank("devices")
                    });
                }
                continue;
            }
            for v in vols {
                out.push(volume_item(&v, Some(&d)));
            }
        }
        // Volumes of their own: phones (MTP), cameras, …
        for v in m.volumes() {
            if v.drive().is_none() {
                out.push(volume_item(&v, None));
            }
        }
        // Mounts without a volume: network shares and the like.
        for mount in m.mounts() {
            if mount.is_shadowed() || mount.volume().is_some() {
                continue;
            }
            out.push(mount_item(&mount, None, None));
        }
        out
    }

    fn header_row(self: &Rc<Self>, id: &'static str) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::new();
        row.set_activatable(false);
        row.set_selectable(false);
        row.set_can_focus(false);
        let title = SECTIONS.iter().find(|(s, _)| *s == id).map_or(id, |(_, t)| t);
        let label = gtk::Label::new(None);
        label.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(&tr(title))));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.style_context().add_class("dim-label");
        // An arrow in front of the title shows whether the section is open.
        let collapsed = config::get().sidebar_collapsed.iter().any(|c| c == id);
        let arrow = gtk::Image::from_icon_name(Some(if collapsed { "pan-end-symbolic" } else { "pan-down-symbolic" }), gtk::IconSize::Menu);
        arrow.style_context().add_class("dim-label");
        arrow.set_margin_start(4);
        let hb = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        hb.set_margin_top(8);
        hb.set_margin_bottom(2);
        hb.add(&arrow);
        hb.add(&label);
        let ev = gtk::EventBox::new();
        ev.set_visible_window(false);
        ev.add(&hb);
        ev.set_tooltip_text(Some(&if collapsed {
            tr("Click to show this section, drag onto another heading to move it there")
        } else {
            tr("Click to hide this section’s entries, drag onto another heading to move it there")
        }));
        // A click collapses or expands the section (in every window, and
        // it's remembered); a drag moves it.
        let w = Rc::downgrade(self);
        ev.connect_button_press_event(move |_, e| {
            if let (Some(s), 1, gdk::EventType::ButtonPress) = (w.upgrade(), e.button(), e.event_type()) {
                s.pressed_header.set(Some(id));
            }
            glib::Propagation::Proceed
        });
        let w = Rc::downgrade(self);
        ev.connect_button_release_event(move |_, e| {
            if let (Some(s), 1) = (w.upgrade(), e.button()) {
                if s.pressed_header.take() == Some(id) {
                    config::update(|c| {
                        if c.sidebar_collapsed.iter().any(|x| x == id) {
                            c.sidebar_collapsed.retain(|x| x != id);
                        } else {
                            c.sidebar_collapsed.push(id.to_string());
                        }
                    });
                    refresh_all();
                }
            }
            glib::Propagation::Proceed
        });
        let targets = [gtk::TargetEntry::new(SECTION_TARGET, gtk::TargetFlags::SAME_APP, 0)];
        ev.drag_source_set(gdk::ModifierType::BUTTON1_MASK, &targets, gdk::DragAction::MOVE);
        let w = Rc::downgrade(self);
        ev.connect_drag_begin(move |ev, ctx| {
            if let Some(s) = w.upgrade() {
                s.pressed_header.set(None);
            }
            drag_picture(ev.upcast_ref(), ctx);
        });
        // A custom target, so the id goes as raw bytes.
        ev.connect_drag_data_get(move |_, _, data, _, _| {
            data.set(&data.target(), 8, id.as_bytes());
        });
        row.add(&ev);
        row
    }

    fn item_row(self: &Rc<Self>, item: &Item, index: usize) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::new();
        row.style_context().add_class("sidebar-row");
        let hb = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        hb.set_margin_start(14);
        hb.set_margin_end(4);
        if let Some(icon) = &item.icon {
            let img = gtk::Image::from_gicon(icon, gtk::IconSize::Menu);
            img.style_context().add_class("sidebar-icon");
            hb.add(&img);
        }
        let label = gtk::Label::new(Some(&item.label));
        label.set_xalign(0.0);
        label.set_ellipsize(pango::EllipsizeMode::End);
        label.set_hexpand(true);
        label.style_context().add_class("sidebar-label");
        hb.add(&label);
        if let Some(kind) = eject_kind(item) {
            let b = gtk::Button::from_icon_name(Some("media-eject-symbolic"), gtk::IconSize::Menu);
            b.set_relief(gtk::ReliefStyle::None);
            b.set_can_focus(false);
            // Kept small (see apply_sidebar_css), so device rows aren't
            // taller than the others.
            b.style_context().add_class("sidebar-button");
            b.set_valign(gtk::Align::Center);
            b.set_tooltip_text(Some(&match kind {
                Removal::Unmount => tr("Unmount"),
                Removal::Stop => tr("Safely remove"),
                Removal::Eject => tr("Eject"),
            }));
            let w = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(s) = w.upgrade() {
                    s.eject(index, eject_kind_at(&s, index));
                }
            });
            hb.add(&b);
        }
        // An event box for middle-/right-clicks and dragging bookmarks.
        let ev = gtk::EventBox::new();
        ev.set_visible_window(false);
        ev.add(&hb);
        if let Some(t) = &item.tooltip {
            ev.set_tooltip_text(Some(t));
        }
        let w = Rc::downgrade(self);
        ev.connect_button_press_event(move |_, e| {
            let Some(s) = w.upgrade() else { return glib::Propagation::Proceed };
            if e.event_type() != gdk::EventType::ButtonPress {
                return glib::Propagation::Proceed;
            }
            match e.button() {
                1 => {
                    *s.pressed.borrow_mut() = s.item_at(index as i32).map(|i| i.signature());
                    glib::Propagation::Proceed
                }
                2 => {
                    s.activate(index as i32, OpenIn::Tab);
                    glib::Propagation::Stop
                }
                3 => {
                    s.context_menu(index, e);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        let w = Rc::downgrade(self);
        ev.connect_button_release_event(move |_, e| {
            if let (Some(s), 1) = (w.upgrade(), e.button()) {
                let pressed = s.pressed.borrow_mut().take();
                if pressed.is_some() && pressed == s.item_at(index as i32).map(|i| i.signature()) {
                    s.activate(index as i32, OpenIn::Current);
                }
            }
            glib::Propagation::Proceed
        });
        // Bookmarks and Computer entries can be dragged to another place in
        // their section (a drag of their own kind, see PLACE_TARGET).
        let dragged = match (&item.bookmark, item.section, &item.key) {
            (Some(uri), _, _) => Some((BOOKMARK_TARGET, uri.clone())),
            (None, "computer", Some(key)) => Some((PLACE_TARGET, key.clone())),
            _ => None,
        };
        if let Some((target, uri)) = dragged {
            let targets = [gtk::TargetEntry::new(target, gtk::TargetFlags::SAME_APP, 0)];
            ev.drag_source_set(gdk::ModifierType::BUTTON1_MASK, &targets, gdk::DragAction::MOVE);
            let w = Rc::downgrade(self);
            ev.connect_drag_begin(move |ev, ctx| {
                // A drag, not a click.
                if let Some(s) = w.upgrade() {
                    s.pressed.borrow_mut().take();
                }
                drag_picture(ev.upcast_ref(), ctx);
            });
            ev.connect_drag_data_get(move |_, _, data, _, _| {
                data.set(&data.target(), 8, uri.as_bytes());
            });
        }
        row.add(&ev);
        row
    }

    fn item_at(&self, index: i32) -> Option<Item> {
        match self.rows.borrow().get(usize::try_from(index).ok()?)? {
            Row::Item(i) => Some(i.clone()),
            Row::Header(_) => None,
        }
    }

    fn section_at(&self, index: i32) -> Option<&'static str> {
        match self.rows.borrow().get(usize::try_from(index).ok()?)? {
            Row::Item(i) => Some(i.section),
            Row::Header(id) => Some(id),
        }
    }

    fn activate(self: &Rc<Self>, index: i32, how: OpenIn) {
        let Some(item) = self.item_at(index) else { return };
        match item.target {
            Target::Location(f) => self.request(Request::Open(f, how)),
            Target::Volume(v) => self.mount(&v, Some(how)),
            Target::Drive(d) => {
                d.poll_for_media(gio::Cancellable::NONE, |_| {});
            }
        }
    }

    fn mount(self: &Rc<Self>, v: &gio::Volume, then: Option<OpenIn>) {
        let op = gtk::MountOperation::new(self.parent_window().as_ref());
        let w = Rc::downgrade(self);
        let vol = v.clone();
        v.mount(gio::MountMountFlags::NONE, Some(&op), gio::Cancellable::NONE, move |res| {
            let Some(s) = w.upgrade() else { return };
            match res {
                Ok(()) => {
                    if let (Some(how), Some(m)) = (then, vol.get_mount()) {
                        s.request(Request::Open(m.root(), how));
                    }
                }
                Err(e) => s.error(&fill(&tr("Unable to mount “{name}”"), &[("name", &vol.name())]), &e),
            }
        });
    }

    fn error(&self, title: &str, e: &glib::Error) {
        // The user cancelled (in a password dialog, say).
        if e.matches(gio::IOErrorEnum::FailedHandled) || e.matches(gio::IOErrorEnum::Cancelled) {
            return;
        }
        util::error_dialog(self.parent_window().as_ref(), title, e.message());
    }

    fn eject(self: &Rc<Self>, index: usize, how: Option<Removal>) {
        let (Some(item), Some(how)) = (self.item_at(index as i32), how) else { return };
        let op = gtk::MountOperation::new(self.parent_window().as_ref());
        let w = Rc::downgrade(self);
        let name = item.label.clone();
        // `failed`: the message's title if it doesn't work, with {name}.
        let done = move |res: Result<(), glib::Error>, failed: String, ok: Option<String>| {
            let Some(s) = w.upgrade() else { return };
            match res {
                Ok(()) => {
                    if let Some(msg) = ok {
                        s.request(Request::Status(msg));
                    }
                }
                Err(e) => s.error(&fill(&failed, &[("name", &name)]), &e),
            }
        };
        let flags = gio::MountUnmountFlags::NONE;
        let none = gio::Cancellable::NONE;
        let label = item.label.clone();
        notify_unmount_progress(&op);
        match how {
            Removal::Unmount => {
                if let Some(m) = &item.mount {
                    // check-placeholders: filled later
                    m.unmount_with_operation(flags, Some(&op), none, move |r| done(r, tr("Unable to unmount “{name}”"), None));
                }
            }
            Removal::Stop => {
                if let Some(d) = &item.drive {
                    let msg = fill(&tr("“{name}” can be removed safely"), &[("name", &d.name())]);
                    // check-placeholders: filled later
                    d.stop(flags, Some(&op), none, move |r| done(r, tr("Unable to safely remove “{name}”"), Some(msg)));
                }
            }
            Removal::Eject => {
                let msg = Some(fill(&tr("“{name}” was ejected"), &[("name", &label)]));
                if let Some(d) = item.drive.as_ref().filter(|d| d.can_eject()) {
                    // check-placeholders: filled later
                    d.eject_with_operation(flags, Some(&op), none, move |r| done(r, tr("Unable to eject “{name}”"), msg));
                } else if let Some(v) = item.volume.as_ref().filter(|v| v.can_eject()) {
                    // check-placeholders: filled later
                    v.eject_with_operation(flags, Some(&op), none, move |r| done(r, tr("Unable to eject “{name}”"), msg));
                } else if let Some(m) = &item.mount {
                    // check-placeholders: filled later
                    m.eject_with_operation(flags, Some(&op), none, move |r| done(r, tr("Unable to eject “{name}”"), msg));
                }
            }
        }
    }

    fn context_menu(self: &Rc<Self>, index: usize, ev: &gdk::EventButton) {
        let Some(item) = self.item_at(index as i32) else { return };
        let menu = gtk::Menu::new();
        let add = |label: &str, f: Box<dyn Fn()>| {
            let mi = gtk::MenuItem::with_mnemonic(&tr(label));
            mi.connect_activate(move |_| f());
            menu.append(&mi);
        };
        let sep = || menu.append(&gtk::SeparatorMenuItem::new());
        let w = Rc::downgrade(self);
        let act = |how: OpenIn| -> Box<dyn Fn()> {
            let w = w.clone();
            Box::new(move || {
                if let Some(s) = w.upgrade() {
                    s.activate(index as i32, how);
                }
            })
        };
        let openable = !matches!(item.target, Target::Drive(_));
        if openable {
            add(n_("_Open"), act(OpenIn::Current));
            add(n_("Open in New _Tab"), act(OpenIn::Tab));
            add(n_("Open in New _Window"), act(OpenIn::Window));
        }
        // Devices.
        let mut device = vec![];
        if let Target::Volume(v) = &item.target {
            if v.can_mount() {
                let (w, v) = (w.clone(), v.clone());
                device.push((n_("_Mount"), Box::new(move || {
                    if let Some(s) = w.upgrade() {
                        s.mount(&v, None);
                    }
                }) as Box<dyn Fn()>));
            }
        }
        let ej = |how: Removal| -> Box<dyn Fn()> {
            let w = w.clone();
            Box::new(move || {
                if let Some(s) = w.upgrade() {
                    s.eject(index, Some(how));
                }
            })
        };
        if item.mount.as_ref().is_some_and(|m| m.can_unmount()) {
            device.push((n_("_Unmount"), ej(Removal::Unmount)));
        }
        // A drive that can be safely removed (a flash drive, a card reader)
        // only gets that: ejecting it would leave it powered on, so there's
        // one way to take it out. Eject is for the rest (discs).
        if removable(&item).is_some_and(|d| d.can_stop()) {
            device.push((n_("_Safely Remove Drive"), ej(Removal::Stop)));
        } else if can_eject(&item) {
            device.push((n_("_Eject"), ej(Removal::Eject)));
        }
        if let Target::Drive(_) = &item.target {
            device.push((n_("_Check for Media"), act(OpenIn::Current)));
        }
        if !device.is_empty() {
            if openable {
                sep();
            }
            for (l, f) in device {
                add(l, f);
            }
        }
        if let Some(uri) = &item.bookmark {
            sep();
            let (w1, u1, l1) = (w.clone(), uri.clone(), item.label.clone());
            add(n_("Re_name…"), Box::new(move || {
                if let Some(s) = w1.upgrade() {
                    s.rename_bookmark(&u1, &l1);
                }
            }));
            let u2 = uri.clone();
            add(n_("_Remove"), Box::new(move || {
                let _ = util::edit_bookmarks(|b| b.retain(|(u, _)| *u != u2));
            }));
        }
        if item.trash {
            sep();
            let w = w.clone();
            add(n_("_Empty Trash"), Box::new(move || {
                let Some(s) = w.upgrade() else { return };
                if let Some(a) = s
                    .parent_window()
                    .and_then(|t| t.downcast::<gtk::ApplicationWindow>().ok())
                    .and_then(|t| t.lookup_action("empty-trash"))
                {
                    a.activate(None);
                }
            }));
        }
        if let Target::Location(f) = &item.target {
            sep();
            let (w, f) = (w.clone(), f.clone());
            add(n_("_Properties"), Box::new(move || {
                if let Some(s) = w.upgrade() {
                    if let Some(p) = s.parent_window() {
                        crate::props::show(&p, vec![f.clone()]);
                    }
                }
            }));
        }
        menu.show_all();
        if let Some(t) = self.parent_window() {
            menu.set_attach_widget(Some(&t));
        }
        menu.popup_at_pointer(Some(ev));
    }

    fn rename_bookmark(&self, uri: &str, current: &str) {
        let d = gtk::Dialog::with_buttons(
            Some(&tr("Rename Bookmark")),
            self.parent_window().as_ref(),
            gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
            &[(tr("_Cancel").as_str(), gtk::ResponseType::Cancel), (tr("_Rename").as_str(), gtk::ResponseType::Accept)],
        );
        d.set_default_response(gtk::ResponseType::Accept);
        let e = gtk::Entry::new();
        e.set_text(current);
        e.set_activates_default(true);
        e.set_width_chars(30);
        let area = d.content_area();
        area.set_border_width(12);
        area.add(&e);
        d.show_all();
        if d.run() == gtk::ResponseType::Accept {
            let name = e.text().trim().to_string();
            let uri = uri.to_string();
            let _ = util::edit_bookmarks(|b| {
                for (u, l) in b.iter_mut() {
                    if *u == uri {
                        *l = name.clone();
                    }
                }
            });
        }
        d.close();
    }

    // ------------------------------------------------------------ dropping

    fn setup_drop(self: &Rc<Self>) {
        let targets = [
            gtk::TargetEntry::new(SECTION_TARGET, gtk::TargetFlags::SAME_APP, 0),
            gtk::TargetEntry::new(BOOKMARK_TARGET, gtk::TargetFlags::SAME_APP, 1),
            gtk::TargetEntry::new(PLACE_TARGET, gtk::TargetFlags::SAME_APP, 3),
            gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::empty(), 2),
        ]
        .into_iter()
        .chain(crate::backgrounds::targets())
        .collect::<Vec<_>>();
        let l = &self.list;
        l.drag_dest_set(gtk::DestDefaults::empty(), &targets, gdk::DragAction::COPY | gdk::DragAction::MOVE | gdk::DragAction::LINK | gdk::DragAction::ASK);
        let w = Rc::downgrade(self);
        l.connect_drag_motion(move |l, ctx, _, y, time| {
            let Some(s) = w.upgrade() else { return false };
            let (row, action) = s.drop_spot(ctx, y);
            match &row {
                Some(r) => l.drag_highlight_row(r),
                None => l.drag_unhighlight_row(),
            }
            ctx.drag_status(action, time);
            true
        });
        l.connect_drag_leave(|l, _, _| l.drag_unhighlight_row());
        let w = Rc::downgrade(self);
        l.connect_drag_drop(move |l, ctx, _, y, time| {
            if let Some(s) = w.upgrade() {
                s.drop_y.set(y);
            }
            match l.drag_dest_find_target(ctx, None) {
                Some(t) => l.drag_get_data(ctx, &t, time),
                None => ctx.drag_finish(false, false, time),
            }
            true
        });
        let w = Rc::downgrade(self);
        l.connect_drag_data_received(move |l, ctx, _, _, data, _, time| {
            l.drag_unhighlight_row();
            let Some(s) = w.upgrade() else { return };
            let ok = s.dropped(ctx, s.drop_y.get(), data);
            ctx.drag_finish(ok, false, time);
        });
    }

    fn kind(&self, ctx: &gdk::DragContext) -> Option<String> {
        self.list.drag_dest_find_target(ctx, None).map(|a| a.name().to_string())
    }

    /// The row a drop at `y` would go to (for highlighting) and the action.
    fn drop_spot(&self, ctx: &gdk::DragContext, y: i32) -> (Option<gtk::ListBoxRow>, gdk::DragAction) {
        let none = (None, gdk::DragAction::empty());
        // A colour or pattern (or Reset): the side pane's background.
        if let Some(k) = self.kind(ctx).filter(|k| crate::backgrounds::is_target(k)) {
            return (None, if k == crate::backgrounds::EMBLEM_TARGET { gdk::DragAction::empty() } else { gdk::DragAction::COPY });
        }
        let Some(row) = self.list.row_at_y(y) else { return none };
        let i = row.index();
        let header = |sec: &str| {
            let rows = self.rows.borrow();
            rows.iter().position(|r| matches!(r, Row::Header(h) if *h == sec)).and_then(|p| self.list.row_at_index(p as i32))
        };
        match self.kind(ctx).as_deref() {
            Some(SECTION_TARGET) => match self.section_at(i) {
                Some(sec) => (header(sec), gdk::DragAction::MOVE),
                None => none,
            },
            Some(BOOKMARK_TARGET) => match self.section_at(i) {
                Some("bookmarks") => (Some(row), gdk::DragAction::MOVE),
                _ => none,
            },
            Some(PLACE_TARGET) => match self.item_at(i) {
                Some(Item { section: "computer", key: Some(_), .. }) => (Some(row), gdk::DragAction::MOVE),
                _ => none,
            },
            Some(_) => match (self.section_at(i), self.item_at(i)) {
                // Folders dropped on the Bookmarks heading become bookmarks.
                (Some("bookmarks"), None) => (Some(row), gdk::DragAction::LINK),
                // So do folders dropped on a bookmark; with Shift they go
                // into its folder.
                (_, Some(Item { bookmark: Some(_), .. })) if bookmarking(self.list.upcast_ref()) => (Some(row), gdk::DragAction::LINK),
                (_, Some(Item { target: Target::Location(f), .. })) => {
                    let a = crate::pane::choose_action(self.list.upcast_ref(), ctx, &f);
                    (Some(row), a)
                }
                _ => none,
            },
            None => none,
        }
    }

    fn dropped(self: &Rc<Self>, ctx: &gdk::DragContext, y: i32, data: &gtk::SelectionData) -> bool {
        use crate::backgrounds::Drop;
        if let Some(k) = self.kind(ctx).filter(|k| crate::backgrounds::is_target(k)) {
            let (color, image) = match crate::backgrounds::parse(&k, &data.data()) {
                Some(Drop::Color(c)) => (c, String::new()),
                Some(Drop::Pattern(p)) => (String::new(), p),
                Some(Drop::Reset) => (String::new(), String::new()),
                _ => return false,
            };
            config::update(|s| {
                s.side_pane_color = color;
                s.side_pane_image = image;
            });
            crate::window::show_side_pane_backgrounds();
            return true;
        }
        let Some(row) = self.list.row_at_y(y) else { return false };
        let i = row.index();
        match self.kind(ctx).as_deref() {
            Some(SECTION_TARGET) => {
                let (Ok(src), Some(dest)) = (String::from_utf8(data.data()), self.section_at(i)) else { return false };
                if src == dest {
                    return false;
                }
                // The dragged section takes the place of the one it's dropped on.
                config::update(|s| {
                    let at = s.sidebar_sections.iter().position(|x| x == dest).unwrap_or(0);
                    s.sidebar_sections.retain(|x| *x != src);
                    s.sidebar_sections.insert(at.min(s.sidebar_sections.len()), src.clone());
                });
                refresh_all();
                true
            }
            Some(PLACE_TARGET) => {
                // Only to another place in the list: the order is all that
                // changes.
                let (Ok(src), Some(Item { section: "computer", key: Some(dest), .. })) = (String::from_utf8(data.data()), self.item_at(i)) else { return false };
                if src == dest {
                    return false;
                }
                let mut order = ordered_places(&computer_keys());
                let at = order.iter().position(|k| *k == dest).unwrap_or(0);
                order.retain(|k| *k != src);
                order.insert(at.min(order.len()), src);
                config::update(|s| s.places_order = order);
                refresh_all();
                true
            }
            Some(BOOKMARK_TARGET) => {
                let Ok(src) = String::from_utf8(data.data()) else { return false };
                let dest = self.item_at(i).and_then(|it| it.bookmark);
                if dest.as_deref() == Some(src.as_str()) {
                    return false;
                }
                let _ = util::edit_bookmarks(|b| {
                    let at = dest.as_ref().and_then(|d| b.iter().position(|(u, _)| u == d)).unwrap_or(0);
                    let Some(from) = b.iter().position(|(u, _)| *u == src) else { return };
                    let moved = b.remove(from);
                    b.insert(at.min(b.len()), moved);
                });
                true
            }
            Some(_) => {
                let uris: Vec<String> = data.uris().iter().map(|u| u.to_string()).collect();
                if uris.is_empty() {
                    return false;
                }
                match (self.section_at(i), self.item_at(i)) {
                    (Some("bookmarks"), None) => {
                        for u in uris.iter().filter(|u| is_folder(u)) {
                            let _ = util::add_bookmark(u);
                        }
                        true
                    }
                    (_, Some(Item { bookmark: Some(at), .. })) if bookmarking(self.list.upcast_ref()) && uris.iter().all(|u| is_folder(u)) => {
                        // In the dropped-on bookmark's place, in their order.
                        let _ = util::edit_bookmarks(|b| {
                            let mut pos = b.iter().position(|(u, _)| *u == at).unwrap_or(b.len());
                            for u in &uris {
                                if !b.iter().any(|(x, _)| x == u) {
                                    b.insert(pos, (u.clone(), String::new()));
                                    pos += 1;
                                }
                            }
                        });
                        true
                    }
                    (_, Some(Item { target: Target::Location(dest), .. })) => {
                        let action = crate::pane::choose_action(self.list.upcast_ref(), ctx, &dest);
                        self.request(Request::Drop { uris, dest, action });
                        true
                    }
                    _ => false,
                }
            }
            None => false,
        }
    }
}

/// Whether a drop on a bookmark adds bookmarks: without Shift (which
/// drops into its folder), unless what's dragged is known not to be all
/// folders.
fn bookmarking(widget: &gtk::Widget) -> bool {
    !crate::pane::pointer_modifiers(widget).contains(gdk::ModifierType::SHIFT_MASK) && crate::pane::drag_is_folders() != Some(false)
}

fn is_folder(uri: &str) -> bool {
    gio::File::for_uri(uri).query_file_type(gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) == gio::FileType::Directory
}

impl Row {
    fn signature(&self) -> String {
        match self {
            Row::Header(id) => format!("header:{id}"),
            Row::Item(i) => i.signature(),
        }
    }
}

impl Item {
    /// Everything its row shows and does, as text: two items with the same
    /// signature have the same row.
    fn signature(&self) -> String {
        let target = match &self.target {
            Target::Location(f) => f.uri().to_string(),
            Target::Volume(v) => format!("volume:{}:{}", v.name(), v.uuid().unwrap_or_default()),
            Target::Drive(d) => format!("drive:{}", d.name()),
        };
        let icon = self.icon.as_ref().and_then(IconExt::to_string).unwrap_or_default();
        let removal = eject_kind(self).map_or(0, |k| k as u8 + 1);
        format!("{}\n{:?}\n{}\n{icon}\n{:?}\n{target}\n{removal}\n{:?}\n{}", self.section, self.key, self.label, self.tooltip, self.bookmark, self.trash)
    }

    fn blank(section: &'static str) -> Item {
        Item {
            section,
            key: None,
            label: String::new(),
            icon: None,
            tooltip: None,
            target: Target::Location(gio::File::for_path("/")),
            mount: None,
            volume: None,
            drive: None,
            bookmark: None,
            trash: false,
        }
    }
}

fn place(f: gio::File, label: &str, icon: &str) -> Item {
    let tooltip = f.path().map(|p| p.to_string_lossy().to_string()).or_else(|| Some(f.uri().to_string()));
    Item {
        key: Some(f.uri().to_string()),
        label: label.to_string(),
        icon: Some(gio::ThemedIcon::new(icon).upcast()),
        tooltip,
        target: Target::Location(f),
        ..Item::blank("computer")
    }
}

/// Home, Desktop, the user's folders (Documents, Music, …), File System and
/// Trash, in the order the user set (see ordered_places).
fn computer_items() -> Vec<Item> {
    let mut out = usual_computer_items();
    let order = ordered_places(&out.iter().filter_map(|i| i.key.clone()).collect::<Vec<_>>());
    out.sort_by_key(|i| i.key.as_ref().and_then(|k| order.iter().position(|o| o == k)));
    out
}

/// The Computer entries in their usual order.
fn usual_computer_items() -> Vec<Item> {
    let home = glib::home_dir();
    let mut out = vec![place(gio::File::for_path(&home), &tr("Home"), "user-home-symbolic")];
    let mut seen = vec![home];
    let mut dir = |path: Option<std::path::PathBuf>, icon: Option<&str>, label: Option<&str>| {
        let Some(path) = path else { return };
        if seen.contains(&path) || !path.is_dir() {
            return;
        }
        seen.push(path.clone());
        let f = gio::File::for_path(&path);
        let info = f.query_info("standard::display-name,standard::symbolic-icon", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE).ok();
        let name = label.map(str::to_string).or_else(|| info.as_ref().map(|i| i.display_name().to_string())).unwrap_or_default();
        let mut item = place(f, &name, icon.unwrap_or("folder-symbolic"));
        if icon.is_none() {
            if let Some(i) = info.and_then(|i| i.symbolic_icon()) {
                item.icon = Some(i);
            }
        }
        out.push(item);
    };
    use glib::UserDirectory as U;
    dir(glib::user_special_dir(U::Desktop), Some("user-desktop-symbolic"), Some(&tr("Desktop")));
    for d in [U::Documents, U::Downloads, U::Music, U::Pictures, U::Videos] {
        dir(glib::user_special_dir(d), None, None);
    }
    // Other folders in user-dirs.dirs (e.g. XDG_PROJECTS_DIR), except the
    // templates and public ones, which GTK leaves out too.
    for p in extra_user_dirs() {
        dir(Some(p), None, None);
    }
    out.push(place(gio::File::for_path("/"), &tr("File System"), "drive-harddisk-symbolic"));
    let trash = gio::File::for_uri("trash:///");
    let full = trash
        .query_info("trash::item-count", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
        .map(|i| i.attribute_uint32("trash::item-count") > 0)
        .unwrap_or(false);
    let mut t = place(trash, &tr("Trash"), if full { "user-trash-full-symbolic" } else { "user-trash-symbolic" });
    t.tooltip = Some(tr("Trash"));
    t.trash = true;
    out.push(t);
    out
}

/// The keys of the Computer entries, in the usual order.
pub fn computer_keys() -> Vec<String> {
    usual_computer_items().into_iter().filter_map(|i| i.key).collect()
}

/// `keys` (the Computer entries, in the usual order) in the order the user
/// set: as saved, with ones not saved yet (a new user folder, say) after
/// the one they follow in the usual order, as new context menu entries go.
pub fn ordered_places(keys: &[String]) -> Vec<String> {
    let saved = config::with(|s| s.places_order.clone());
    let defaults: Vec<&str> = keys.iter().map(String::as_str).collect();
    crate::actions::menu_order(&saved, &defaults, &[])
}

fn extra_user_dirs() -> Vec<std::path::PathBuf> {
    let text = std::fs::read_to_string(config::config_home().join("user-dirs.dirs")).unwrap_or_default();
    parse_user_dirs(&text, &glib::home_dir())
}

/// The folders of a user-dirs.dirs other than the usual ones.
fn parse_user_dirs(text: &str, home: &std::path::Path) -> Vec<std::path::PathBuf> {
    const KNOWN: &[&str] = &["DESKTOP", "DOWNLOAD", "TEMPLATES", "PUBLICSHARE", "DOCUMENTS", "MUSIC", "PICTURES", "VIDEOS"];
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.trim().split_once('=')?;
            let name = k.strip_prefix("XDG_")?.strip_suffix("_DIR")?;
            if KNOWN.contains(&name) {
                return None;
            }
            let v = v.trim().trim_matches('"');
            Some(match v.strip_prefix("$HOME") {
                Some(rest) => home.join(rest.trim_start_matches('/')),
                None => std::path::PathBuf::from(v),
            })
        })
        .collect()
}

fn bookmark_items() -> Vec<Item> {
    util::read_bookmarks()
        .into_iter()
        .filter_map(|(uri, label)| {
            let f = gio::File::for_uri(&uri);
            // Bookmarks of local folders that are gone are left out.
            if f.is_native() && !f.query_exists(gio::Cancellable::NONE) {
                return None;
            }
            let icon = if f.is_native() { "folder-symbolic" } else { "folder-remote-symbolic" };
            let mut item = place(f, &label, icon);
            item.section = "bookmarks";
            item.key = None;
            item.bookmark = Some(uri);
            Some(item)
        })
        .collect()
}

fn volume_item(v: &gio::Volume, d: Option<&gio::Drive>) -> Item {
    if let Some(m) = v.get_mount() {
        return mount_item(&m, Some(v), d);
    }
    let name = v.name().to_string();
    Item {
        icon: Some(v.symbolic_icon()),
        tooltip: Some(fill(&tr("{name} (not mounted)"), &[("name", &name)])),
        label: name,
        target: Target::Volume(v.clone()),
        volume: Some(v.clone()),
        drive: d.cloned(),
        ..Item::blank("devices")
    }
}

fn mount_item(m: &gio::Mount, v: Option<&gio::Volume>, d: Option<&gio::Drive>) -> Item {
    let name = m.name().to_string();
    let root = m.root();
    let tooltip = root.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| root.uri().to_string());
    Item {
        icon: Some(m.symbolic_icon()),
        tooltip: Some(tooltip),
        label: name,
        target: Target::Location(root),
        mount: Some(m.clone()),
        volume: v.cloned(),
        drive: d.cloned().or_else(|| m.drive()),
        ..Item::blank("devices")
    }
}

#[derive(Clone, Copy)]
enum Removal {
    Unmount,
    /// Power the drive off ("safely remove").
    Stop,
    Eject,
}

/// The item's drive if it's removable (a USB stick, a card reader), not an
/// internal disk, which must never be powered off from here.
fn removable(item: &Item) -> Option<&gio::Drive> {
    item.drive.as_ref().filter(|d| d.is_removable())
}

fn can_eject(item: &Item) -> bool {
    removable(item).is_some_and(|d| d.can_eject())
        || item.volume.as_ref().is_some_and(|v| v.can_eject())
        || item.mount.as_ref().is_some_and(|m| m.can_eject())
}

/// What the button next to a device does: by default it safely removes
/// (or ejects) removable drives; with "only unmount" set, it just unmounts.
fn eject_kind(item: &Item) -> Option<Removal> {
    let can_unmount = item.mount.as_ref().is_some_and(|m| m.can_unmount());
    if config::with(|s| s.unmount_only) {
        return can_unmount.then_some(Removal::Unmount);
    }
    if removable(item).is_some_and(|d| d.can_stop()) {
        Some(Removal::Stop)
    } else if can_eject(item) {
        Some(Removal::Eject)
    } else if can_unmount {
        Some(Removal::Unmount)
    } else {
        None
    }
}

fn eject_kind_at(s: &Sidebar, index: usize) -> Option<Removal> {
    s.item_at(index as i32).as_ref().and_then(eject_kind)
}

/// Drag a picture of the row (or heading).
pub fn drag_picture(w: &gtk::Widget, ctx: &gdk::DragContext) {
    let (width, height) = (w.allocated_width(), w.allocated_height());
    if let Ok(surface) = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, width, height) {
        if let Ok(cr) = gtk::cairo::Context::new(&surface) {
            w.draw(&cr);
        }
        surface.set_device_offset(-8.0, -(height as f64) / 2.0);
        ctx.drag_set_icon_surface(&surface);
    }
}

/// The desktop's own setting for mounting inserted media. Arkha's "Mount
/// drives and media when they're inserted" is that same setting, as Caja's
/// is MATE's: changing it in one place changes it in the other, and Arkha
/// never changes it on its own, so nothing is left to undo when Arkha is
/// removed. Without a desktop (a plain window manager) Arkha keeps its own.
#[derive(Clone, Copy, Debug, PartialEq)]
enum DesktopMount {
    /// GNOME, Cinnamon and MATE (Caja): the schema, and the desktop's name.
    Gsettings(&'static str, &'static str),
    /// Xfce: thunar-volman's switches for drives and for media, in xfconf.
    Xfce,
    /// KDE Plasma's device automounter.
    Kde,
    /// LXQt / LXDE: their file managers, PCManFM-Qt and PCManFM, mount
    /// media, set in their config files (one per profile). They read these
    /// when they start, so a change there applies from the next login.
    Pcmanfm { qt: bool },
}

/// The desktop `XDG_CURRENT_DESKTOP` names (the first one known, as in
/// `Budgie:GNOME`), whether or not its settings are installed.
fn desktop_kind(current: &str) -> Option<DesktopMount> {
    current.split(':').find_map(|d| match d.to_ascii_lowercase().as_str() {
        "x-cinnamon" | "cinnamon" => Some(DesktopMount::Gsettings("org.cinnamon.desktop.media-handling", "Cinnamon")),
        "mate" => Some(DesktopMount::Gsettings("org.mate.media-handling", "MATE")),
        "gnome" | "gnome-classic" | "gnome-flashback" | "unity" => Some(DesktopMount::Gsettings("org.gnome.desktop.media-handling", "GNOME")),
        "xfce" => Some(DesktopMount::Xfce),
        "kde" => Some(DesktopMount::Kde),
        "lxqt" => Some(DesktopMount::Pcmanfm { qt: true }),
        "lxde" => Some(DesktopMount::Pcmanfm { qt: false }),
        _ => None,
    })
}

/// A program's output, if it ran and succeeded.
fn output(prog: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(prog)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn first_program(names: &[&'static str]) -> Option<&'static str> {
    names.iter().copied().find(|n| glib::find_program_in_path(n).is_some())
}

const KDE_AUTOMOUNT: [&str; 6] = ["--file", "kded_device_automounterrc", "--group", "General", "--key", "AutomountEnabled"];
const XFCE_AUTOMOUNT: [&str; 2] = ["/automount-drives/enabled", "/automount-media/enabled"];

impl DesktopMount {
    /// The current desktop's, if it has one and its settings are there.
    fn get() -> Option<DesktopMount> {
        thread_local! {
            static FOUND: std::cell::OnceCell<Option<DesktopMount>> = const { std::cell::OnceCell::new() };
        }
        FOUND.with(|f| {
            *f.get_or_init(|| {
                let d = desktop_kind(&std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default())?;
                let there = match d {
                    DesktopMount::Gsettings(schema, _) => gio::SettingsSchemaSource::default()
                        .and_then(|src| src.lookup(schema, true))
                        .is_some_and(|s| s.has_key("automount")),
                    DesktopMount::Xfce => first_program(&["xfconf-query"]).is_some(),
                    DesktopMount::Kde => Self::kde_tools().is_some(),
                    DesktopMount::Pcmanfm { .. } => true,
                };
                there.then_some(d)
            })
        })
    }

    fn name(self) -> &'static str {
        match self {
            DesktopMount::Gsettings(_, name) => name,
            DesktopMount::Xfce => "Xfce",
            DesktopMount::Kde => "KDE Plasma",
            DesktopMount::Pcmanfm { qt: true } => "LXQt",
            DesktopMount::Pcmanfm { qt: false } => "LXDE",
        }
    }

    fn kde_tools() -> Option<(&'static str, &'static str)> {
        Some((first_program(&["kreadconfig6", "kreadconfig5"])?, first_program(&["kwriteconfig6", "kwriteconfig5"])?))
    }

    /// PCManFM(-Qt)'s config files: every profile's, or the desktop's usual
    /// profile's when there's none yet. Then the section, key and values.
    fn pcmanfm(qt: bool) -> (Vec<std::path::PathBuf>, &'static str, &'static str, [&'static str; 2]) {
        let (app, file, profile) = if qt { ("pcmanfm-qt", "settings.conf", "lxqt") } else { ("pcmanfm", "pcmanfm.conf", "LXDE") };
        let dir = config::config_home().join(app);
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .map(|d| d.flatten().map(|e| e.path().join(file)).filter(|p| p.is_file()).collect())
            .unwrap_or_default();
        files.sort();
        if files.is_empty() {
            files.push(dir.join(profile).join(file));
        }
        if qt {
            (files, "Volume", "MountRemovable", ["false", "true"])
        } else {
            (files, "volume", "mount_removable", ["0", "1"])
        }
    }

    fn is_on(self) -> bool {
        match self {
            DesktopMount::Gsettings(schema, _) => gio::Settings::new(schema).boolean("automount"),
            // thunar-volman's switches are off unless set.
            DesktopMount::Xfce => XFCE_AUTOMOUNT
                .iter()
                .any(|p| output("xfconf-query", &["-c", "thunar-volman", "-p", p]).as_deref() == Some("true")),
            DesktopMount::Kde => Self::kde_tools().is_none_or(|(read, _)| {
                output(read, &[&KDE_AUTOMOUNT[..], &["--default", "true"]].concat()).as_deref() != Some("false")
            }),
            // Missing, the key is on.
            DesktopMount::Pcmanfm { qt } => {
                let (files, section, key, [off, _]) = Self::pcmanfm(qt);
                let text = std::fs::read_to_string(&files[0]).unwrap_or_default();
                ini_get(&text, section, key).as_deref() != Some(off)
            }
        }
    }

    fn set(self, on: bool) {
        let word = if on { "true" } else { "false" };
        match self {
            DesktopMount::Gsettings(schema, _) => {
                let _ = gio::Settings::new(schema).set_boolean("automount", on);
                gio::Settings::sync();
            }
            DesktopMount::Xfce => {
                for p in XFCE_AUTOMOUNT {
                    output("xfconf-query", &["-c", "thunar-volman", "-p", p, "-n", "-t", "bool", "-s", word]);
                }
            }
            DesktopMount::Kde => {
                if let Some((_, write)) = Self::kde_tools() {
                    output(write, &[&KDE_AUTOMOUNT[..], &["--type", "bool", word]].concat());
                }
            }
            DesktopMount::Pcmanfm { qt } => {
                let (files, section, key, values) = Self::pcmanfm(qt);
                for path in files {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(&path, ini_set(&text, section, key, values[on as usize]));
                }
            }
        }
    }
}

/// Whether inserted media are mounted: the desktop's own setting, or
/// Arkha's without a desktop.
pub fn automount_on() -> bool {
    DesktopMount::get().map_or_else(|| config::with(|s| s.automount), DesktopMount::is_on)
}

/// Sets that, in the desktop's settings too.
pub fn set_automount(on: bool) {
    if let Some(d) = DesktopMount::get() {
        d.set(on);
    }
    config::update(|s| s.automount = on);
}

/// The desktop's media-handling settings (GNOME, Cinnamon, MATE), which
/// also say what to do with inserted media (see autorun.rs).
pub fn media_settings() -> Option<gio::Settings> {
    match DesktopMount::get()? {
        DesktopMount::Gsettings(schema, _) => Some(gio::Settings::new(schema)),
        _ => None,
    }
}

/// The desktop whose setting that is, if any.
pub fn automount_desktop() -> Option<&'static str> {
    DesktopMount::get().map(DesktopMount::name)
}

fn ini_section(line: &str) -> Option<&str> {
    line.trim().strip_prefix('[')?.strip_suffix(']')
}

fn ini_get(text: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in text.lines() {
        if let Some(s) = ini_section(line) {
            inside = s == section;
        } else if inside {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// `text` with `key=value` in `[section]`: the existing line replaced, or a
/// new one right under the section's header (the section added if needed).
fn ini_set(text: &str, section: &str, key: &str, value: &str) -> String {
    let mut out = vec![];
    let (mut inside, mut found_section, mut done) = (false, false, false);
    for line in text.lines() {
        if let Some(s) = ini_section(line) {
            inside = s == section;
            out.push(line.to_string());
            if inside {
                found_section = true;
                // Placeholder for a new key, dropped below if it's found.
                out.push(String::from("\0"));
            }
            continue;
        }
        if inside && !done && line.split_once('=').is_some_and(|(k, _)| k.trim() == key) {
            out.push(format!("{key}={value}"));
            done = true;
            continue;
        }
        out.push(line.to_string());
    }
    let new_line = format!("{key}={value}");
    let mut placed = done;
    let mut out: Vec<String> = out
        .into_iter()
        .filter_map(|l| {
            if l != "\0" {
                return Some(l);
            }
            // Only the first [section] gets the new key.
            if placed {
                None
            } else {
                placed = true;
                Some(new_line.clone())
            }
        })
        .collect();
    if !found_section {
        if out.last().is_some_and(|l| !l.is_empty()) {
            out.push(String::new());
        }
        out.push(format!("[{section}]"));
        out.push(new_line);
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// Some drives (a VM's virtual CD drive, for one) never report a disc being
/// taken out: the system only notices when the drive is next opened, so the
/// disc stayed in the list. Drives with removable media are therefore opened
/// for a moment every two seconds while they hold something; with O_NONBLOCK
/// that only makes the kernel check, the disc isn't read.
fn poll_removable_media(monitor: &gio::VolumeMonitor) {
    use std::os::unix::fs::OpenOptionsExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let busy = Arc::new(AtomicBool::new(false));
    let monitor = monitor.clone();
    glib::timeout_add_seconds_local(2, move || {
        let devices: Vec<String> = monitor
            .connected_drives()
            .iter()
            .filter(|d| d.is_media_removable() && d.has_volumes())
            .filter_map(|d| d.identifier("unix-device"))
            .map(|s| s.to_string())
            .collect();
        // Off the main thread: a slow drive mustn't freeze the window.
        if !devices.is_empty() && !busy.swap(true, Ordering::SeqCst) {
            let busy = busy.clone();
            std::thread::spawn(move || {
                for dev in devices {
                    let _ = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(&dev);
                }
                busy.store(false, Ordering::SeqCst);
            });
        }
        glib::ControlFlow::Continue
    });
}

/// Programs run on their own (often with a plain window manager) that mount
/// inserted media. They have no setting to switch off, only not starting
/// them, so Arkha can't do that for the user; it says so, once for each.
const AUTOMOUNTERS: &[&str] = &["udiskie", "devmon", "ldm", "udisks-glue"];

fn running_automounters() -> Vec<&'static str> {
    let mut found = vec![];
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Ok(comm) = std::fs::read_to_string(e.path().join("comm")) else { continue };
        if let Some(a) = AUTOMOUNTERS.iter().find(|a| **a == comm.trim()) {
            if !found.contains(a) {
                found.push(*a);
            }
        }
    }
    found
}

fn notice_automounters() {
    let seen = config::get().automounters_noticed;
    let new: Vec<&str> = running_automounters().into_iter().filter(|a| !seen.iter().any(|s| s == a)).collect();
    if new.is_empty() {
        return;
    }
    config::update(|s| s.automounters_noticed.extend(new.iter().map(|a| a.to_string())));
    let parent = gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::ApplicationWindow>().ok())
        .find(|w| w.is_visible());
    // TRANSLATORS: joins the names of two programs: "udiskie and Thunar".
    let names = new.join(&tr(" and "));
    let n = new.len() as u64;
    let d = gtk::MessageDialog::new(
        parent.as_ref(),
        gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Info,
        gtk::ButtonsType::Ok,
        &fill(&trn("{names} is mounting drives too", "{names} are mounting drives too", n), &[("names", &names)]),
    );
    d.set_secondary_text(Some(&fill(
        &trn(
            "It runs separately from Arkha and mounts inserted drives and media by itself, whatever Arkha’s settings are, and Arkha can’t switch it off.\n\nTo leave mounting to Arkha, stop {names} from starting with your session (for example, remove it from your window manager’s autostart) and log in again.\n\nThis message isn’t shown again.",
            "They run separately from Arkha and mount inserted drives and media by themselves, whatever Arkha’s settings are, and Arkha can’t switch them off.\n\nTo leave mounting to Arkha, stop {names} from starting with your session (for example, remove them from your window manager’s autostart) and log in again.\n\nThis message isn’t shown again.",
            n,
        ),
        &[("names", &names)],
    )));
    d.connect_response(|d, _| d.close());
    d.show();
}

/// Mounts removable media when they're inserted (per the settings). Runs
/// once for the whole program; only media inserted while Arkha runs.
pub fn init_automount() {
    thread_local! {
        static MONITOR: RefCell<Option<gio::VolumeMonitor>> = const { RefCell::new(None) };
    }
    MONITOR.with(|m| {
        if m.borrow().is_some() {
            return;
        }
        // Once the first window is up.
        glib::timeout_add_local_once(std::time::Duration::from_secs(2), notice_automounters);
        let monitor = gio::VolumeMonitor::get();
        poll_removable_media(&monitor);
        monitor.connect_volume_added(|_, v| {
            if !v.should_automount() || !automount_on() {
                return;
            }
            let mount = |v: &gio::Volume| {
                if !v.can_mount() || v.get_mount().is_some() {
                    return;
                }
                let op = gtk::MountOperation::new(None::<&gtk::Window>);
                // Errors are left alone: another program may have mounted
                // it meanwhile. Mounted, what's on it may start a program or
                // open a window (see autorun.rs).
                let vol = v.clone();
                v.mount(gio::MountMountFlags::NONE, Some(&op), gio::Cancellable::NONE, move |r| {
                    if let (Ok(()), Some(m)) = (r, vol.get_mount()) {
                        crate::autorun::on_mounted(&m);
                    }
                });
            };
            // A desktop mounts media itself (and may open a window for them,
            // per its settings), so it gets the first go; Arkha mounts what
            // it left, in case its automounter isn't running.
            if DesktopMount::get().is_some() {
                let v = v.clone();
                glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || mount(&v));
            } else {
                mount(v);
            }
        });
        *m.borrow_mut() = Some(monitor);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_editing() {
        let text = "[Behavior]\nSingleClick=false\n\n[Volume]\nAutoRun=true\nMountRemovable=true\n\n[Window]\nWidth=640\n";
        assert_eq!(ini_get(text, "Volume", "MountRemovable").as_deref(), Some("true"));
        assert_eq!(ini_get(text, "Volume", "MountOnStartup"), None);
        assert_eq!(ini_get(text, "Window", "AutoRun"), None);
        // An existing key is replaced in place, a missing one goes under the
        // section's header; nothing else changes.
        let t = ini_set(text, "Volume", "MountRemovable", "false");
        let t = ini_set(&t, "Volume", "MountOnStartup", "false");
        assert_eq!(t, "[Behavior]\nSingleClick=false\n\n[Volume]\nMountOnStartup=false\nAutoRun=true\nMountRemovable=false\n\n[Window]\nWidth=640\n");
        // A missing section is added at the end.
        assert_eq!(ini_set("[desktop]\nwallpaper=x\n", "volume", "mount_removable", "0"), "[desktop]\nwallpaper=x\n\n[volume]\nmount_removable=0\n");
        assert_eq!(ini_set("", "volume", "mount_removable", "0"), "[volume]\nmount_removable=0\n");
    }

    #[test]
    fn extra_user_folders() {
        let home = std::path::Path::new("/home/u");
        let text = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_PROJECTS_DIR=\"$HOME/Projects\"\n  XDG_BOOKS_DIR=\"/srv/books\"\nXDG_MUSIC_DIR=\"$HOME/Music\"\nNOT_A_DIR=x\nXDG_HOME_DIR=\"$HOME/\"\n";
        assert_eq!(parse_user_dirs(text, home), [home.join("Projects"), "/srv/books".into(), home.to_path_buf()]);
        assert!(parse_user_dirs("", home).is_empty());
    }

    #[test]
    fn pcmanfm_settings_are_shared() {
        // In the tests' settings folder (see config::config_home).
        let dir = config::config_home().join("pcmanfm-qt");
        let _ = std::fs::remove_dir_all(&dir);
        let qt = DesktopMount::Pcmanfm { qt: true };
        // Missing, it's on (PCManFM-Qt's default); set, a new profile file.
        assert!(qt.is_on());
        qt.set(false);
        let lxqt = dir.join("lxqt/settings.conf");
        assert_eq!(std::fs::read_to_string(&lxqt).unwrap(), "[Volume]\nMountRemovable=false\n");
        assert!(!qt.is_on());
        // Every profile changes, the rest of each file stays.
        std::fs::create_dir_all(dir.join("other")).unwrap();
        std::fs::write(dir.join("other/settings.conf"), "[Behavior]\nSingleClick=true\n\n[Volume]\nMountRemovable=false\nAutoRun=true\n").unwrap();
        qt.set(true);
        assert!(qt.is_on());
        assert_eq!(std::fs::read_to_string(&lxqt).unwrap(), "[Volume]\nMountRemovable=true\n");
        assert_eq!(std::fs::read_to_string(dir.join("other/settings.conf")).unwrap(), "[Behavior]\nSingleClick=true\n\n[Volume]\nMountRemovable=true\nAutoRun=true\n");
        let _ = std::fs::remove_dir_all(&dir);
        // PCManFM writes 0 and 1.
        let lx = DesktopMount::Pcmanfm { qt: false };
        let dir = config::config_home().join("pcmanfm");
        let _ = std::fs::remove_dir_all(&dir);
        lx.set(false);
        assert_eq!(std::fs::read_to_string(dir.join("LXDE/pcmanfm.conf")).unwrap(), "[volume]\nmount_removable=0\n");
        assert!(!lx.is_on());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn desktop_names() {
        assert_eq!(desktop_kind("X-Cinnamon").map(DesktopMount::name), Some("Cinnamon"));
        assert_eq!(desktop_kind("Budgie:GNOME").map(DesktopMount::name), Some("GNOME"));
        assert_eq!(desktop_kind("ubuntu:GNOME").map(DesktopMount::name), Some("GNOME"));
        assert_eq!(desktop_kind("MATE"), Some(DesktopMount::Gsettings("org.mate.media-handling", "MATE")));
        assert_eq!(desktop_kind("XFCE"), Some(DesktopMount::Xfce));
        assert_eq!(desktop_kind("KDE"), Some(DesktopMount::Kde));
        assert_eq!(desktop_kind("LXQt"), Some(DesktopMount::Pcmanfm { qt: true }));
        assert_eq!(desktop_kind("LXDE"), Some(DesktopMount::Pcmanfm { qt: false }));
        assert_eq!(desktop_kind("i3"), None);
        assert_eq!(desktop_kind(""), None);
    }

    #[test]
    fn random_ini_files() {
        let chars: Vec<char> = "[]=ab \n#;".chars().collect();
        let names: Vec<char> = "abc".chars().collect();
        let values: Vec<char> = "xyz01äß".chars().collect();
        crate::testutil::cases(3000, |r| {
            let text = r.text_of(&chars, 60);
            let section = r.text_of(&names, 3) + "s";
            let key = r.text_of(&names, 3) + "k";
            let value = r.text_of(&values, 5);
            let set = ini_set(&text, &section, &key, &value);
            assert_eq!(ini_get(&set, &section, &key).as_deref(), Some(value.as_str()), "{text:?} → {set:?}");
            // Setting it again changes nothing.
            assert_eq!(ini_set(&set, &section, &key, &value), set);
        });
    }
}
