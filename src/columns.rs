//! The columns view (like macOS Finder's): the folders above the open one in
//! columns to its left, each with the folder on the way highlighted, and the
//! selected folder's contents (or the selected file's preview) in a column
//! to its right. The open folder's own column is the pane's list view, so
//! everything there works as in the list view. These side columns list a
//! folder (kept up to date), and their clicks, context menus and drops are
//! handed to the pane (pane.rs): a click goes to that folder with the item
//! selected, a right-click does that and opens the context menu, items can
//! be dragged out, and things dropped on a folder row or a column go there.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gio::prelude::*;
use glib::translate::IntoGlib;
use crate::i18n::tr;
use gtk::prelude::*;

use crate::pane::ModelGet;
use crate::{config, util};

/// Width of a column, the open folder's included.
pub const WIDTH: i32 = 240;

const C_ICON: u32 = 0;
const C_NAME: u32 = 1;
const C_URI: u32 = 2;
const C_IS_DIR: u32 = 3;
const C_KEY: u32 = 4;

/// Gives a column (side ones, and the open folder's list in the columns
/// view) a line on its right, so the columns are told apart even when one
/// is empty.
pub fn style_column(w: &gtk::ScrolledWindow, on: bool) {
    thread_local! {
        static CSS: gtk::CssProvider = {
            let css = gtk::CssProvider::new();
            let _ = css.load_from_data(b"scrolledwindow.arkha-column { border-right: 1px solid @borders; }");
            css
        };
    }
    let ctx = w.style_context();
    if on {
        CSS.with(|c| ctx.add_provider(c, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION));
        ctx.add_class("arkha-column");
    } else {
        ctx.remove_class("arkha-column");
    }
}

/// Draws "Empty folder" in a column whose folder has nothing in it.
pub fn draw_empty_hint(view: &gtk::TreeView, cr: &gtk::cairo::Context) {
    let layout = view.create_pango_layout(Some(&tr("Empty folder")));
    let (w, _) = layout.pixel_size();
    // In the theme's colour for dimmed text.
    let ctx = view.style_context();
    ctx.save();
    ctx.set_state(gtk::StateFlags::INSENSITIVE);
    gtk::render_layout(&ctx, cr, ((view.allocated_width() - w) / 2).max(0) as f64, 24.0, &layout);
    ctx.restore();
}

/// Go to `folder` with `uri` selected.
pub type ClickHook = Box<dyn Fn(&gio::File, &str)>;
/// Go to `folder` with `uri` (or nothing) selected and open the context
/// menu there.
pub type ContextHook = Box<dyn Fn(&gio::File, Option<&str>, &gdk::EventButton)>;
/// Files were dropped on a folder: (uris, folder, action).
pub type DropHook = Box<dyn Fn(Vec<String>, gio::File, gdk::DragAction)>;

/// What the columns ask of the pane.
pub struct Hooks {
    pub click: ClickHook,
    pub context: ContextHook,
    pub drop: DropHook,
}

pub struct Column {
    pub widget: gtk::ScrolledWindow,
    pub folder: gio::File,
    view: gtk::TreeView,
    store: gtk::ListStore,
    /// The row to show as selected (the folder on the way), once loaded.
    highlight: RefCell<Option<String>>,
    loaded: Cell<bool>,
    show_hidden: bool,
    /// The item a left button press started on: a click goes there on
    /// release, unless it became a drag.
    pressed: RefCell<Option<String>>,
    _monitor: RefCell<Option<gio::FileMonitor>>,
    /// Bumped per (re)load, so an older one stops filling in rows.
    gen: Cell<u64>,
    reload_queued: Cell<bool>,
}

impl Column {
    /// `show_hidden` and `folders_first` as in the pane it's part of.
    pub fn new(folder: &gio::File, hooks: Rc<Hooks>, show_hidden: bool, folders_first: bool) -> Rc<Column> {
        let store = gtk::ListStore::new(&[gio::Icon::static_type(), String::static_type(), String::static_type(), bool::static_type(), String::static_type()]);
        // Folders first (as set), then by name.
        store.set_sort_func(gtk::SortColumn::Index(C_KEY), move |m, a, b| {
            let (da, db) = (m.gv::<bool>(a, C_IS_DIR as i32), m.gv::<bool>(b, C_IS_DIR as i32));
            if folders_first && da != db {
                return if da { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
            }
            m.gv::<String>(a, C_KEY as i32).cmp(&m.gv::<String>(b, C_KEY as i32))
        });
        store.set_sort_column_id(gtk::SortColumn::Index(C_KEY), gtk::SortType::Ascending);
        let view = gtk::TreeView::with_model(&store);
        view.set_headers_visible(false);
        view.set_enable_search(false);
        let col = gtk::TreeViewColumn::new();
        let size = if config::with(|s| s.list_size) >= 24 { gtk::IconSize::LargeToolbar } else { gtk::IconSize::Menu };
        let icon = gtk::CellRendererPixbuf::new();
        // The property is a plain number, not a GtkIconSize.
        icon.set_property("stock-size", size.into_glib() as u32);
        let text = gtk::CellRendererText::new();
        text.set_property("ellipsize", pango::EllipsizeMode::End);
        let arrow = gtk::CellRendererPixbuf::new();
        arrow.set_property("icon-name", "go-next-symbolic");
        CellLayoutExt::pack_start(&col, &icon, false);
        CellLayoutExt::pack_start(&col, &text, true);
        CellLayoutExt::pack_end(&col, &arrow, false);
        CellLayoutExt::add_attribute(&col, &icon, "gicon", C_ICON as i32);
        CellLayoutExt::add_attribute(&col, &text, "text", C_NAME as i32);
        CellLayoutExt::add_attribute(&col, &arrow, "visible", C_IS_DIR as i32);
        view.append_column(&col);
        let widget = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        widget.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        widget.set_size_request(WIDTH, -1);
        widget.add(&view);
        widget.show_all();
        style_column(&widget, true);
        {
            let store = store.clone();
            view.connect_local("draw", true, move |vals| {
                let v = vals[0].get::<gtk::TreeView>().ok()?;
                let cr = vals[1].get::<gtk::cairo::Context>().ok()?;
                if store.iter_first().is_none() {
                    draw_empty_hint(&v, &cr);
                }
                Some(false.to_value())
            });
        }
        let c = Rc::new(Column {
            widget,
            folder: folder.clone(),
            view,
            store,
            highlight: RefCell::new(None),
            loaded: Cell::new(false),
            show_hidden,
            pressed: RefCell::new(None),
            _monitor: RefCell::new(None),
            gen: Cell::new(0),
            reload_queued: Cell::new(false),
        });
        c.setup_clicks(hooks.clone());
        c.setup_dnd(hooks);
        // Kept up to date: reloaded (once per burst of changes) when the
        // folder changes.
        if let Ok(m) = folder.monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
            let w = Rc::downgrade(&c);
            m.connect_changed(move |_, _, _, _| {
                if let Some(c) = w.upgrade() {
                    c.queue_reload();
                }
            });
            *c._monitor.borrow_mut() = Some(m);
        }
        c.load();
        c
    }

    /// The URI of the item at (`x`, `y`) in the view's bin window.
    fn uri_at(&self, x: f64, y: f64) -> Option<String> {
        let path = self.view.path_at_pos(x as i32, y as i32).and_then(|(p, _, _, _)| p)?;
        let it = self.store.iter(&path)?;
        Some(self.store.gv::<String>(&it, C_URI as i32))
    }

    fn setup_clicks(self: &Rc<Self>, hooks: Rc<Hooks>) {
        let w = Rc::downgrade(self);
        let h = hooks.clone();
        self.view.connect_button_press_event(move |v, ev| {
            let Some(c) = w.upgrade() else { return glib::Propagation::Proceed };
            if ev.event_type() != gdk::EventType::ButtonPress || ev.window() != v.bin_window() {
                return glib::Propagation::Proceed;
            }
            let (x, y) = ev.position();
            match ev.button() {
                // Remembered; the view selects the row (and may start a drag).
                1 => {
                    *c.pressed.borrow_mut() = c.uri_at(x, y);
                    glib::Propagation::Proceed
                }
                3 => {
                    (h.context)(&c.folder, c.uri_at(x, y).as_deref(), ev);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        let w = Rc::downgrade(self);
        self.view.connect_button_release_event(move |_, ev| {
            if let (Some(c), 1) = (w.upgrade(), ev.button()) {
                let taken = c.pressed.borrow_mut().take();
                if let Some(uri) = taken {
                    (hooks.click)(&c.folder, &uri);
                }
            }
            glib::Propagation::Proceed
        });
    }

    fn setup_dnd(self: &Rc<Self>, hooks: Rc<Hooks>) {
        let v = &self.view;
        // Items can be dragged out of a column…
        crate::pane::enable_drag_source_tv(v);
        let w = Rc::downgrade(self);
        v.connect_drag_begin(move |_, _| {
            if let Some(c) = w.upgrade() {
                // A drag, not a click.
                c.pressed.borrow_mut().take();
                crate::pane::set_drag_source_dir(Some(c.folder.uri().to_string()));
            }
        });
        v.connect_drag_end(|_, _| crate::pane::set_drag_source_dir(None));
        // The files are moved by the file operations; GTK mustn't remove
        // the row itself (the monitor reloads the column).
        v.connect_drag_data_delete(|v, _| v.stop_signal_emission_by_name("drag-data-delete"));
        let w = Rc::downgrade(self);
        v.connect_drag_data_get(move |v, _, sel, _, _| {
            let Some(c) = w.upgrade() else { return };
            let (paths, _) = v.selection().selected_rows();
            let uris: Vec<String> = paths.iter().filter_map(|p| c.store.iter(p)).map(|it| c.store.gv::<String>(&it, C_URI as i32)).collect();
            let refs: Vec<&str> = uris.iter().map(|s| s.as_str()).collect();
            sel.set_uris(&refs);
        });
        // …and things dropped on a folder row go into it, anywhere else in
        // the column into the column's folder.
        v.drag_dest_set(gtk::DestDefaults::empty(), &crate::pane::drop_targets(), crate::pane::dnd_actions());
        let w = Rc::downgrade(self);
        v.connect_drag_motion(move |v, ctx, x, y, time| {
            let Some(c) = w.upgrade() else { return false };
            if v.drag_dest_find_target(ctx, None).is_none() {
                ctx.drag_status(gdk::DragAction::empty(), time);
                return true;
            }
            let (dest, row) = c.drop_target(x, y);
            v.set_drag_dest_row(row.as_ref(), gtk::TreeViewDropPosition::IntoOrBefore);
            ctx.drag_status(crate::pane::choose_action(v.upcast_ref(), ctx, &dest), time);
            true
        });
        v.connect_drag_leave(|v, _, _| v.set_drag_dest_row(None, gtk::TreeViewDropPosition::IntoOrBefore));
        let w = Rc::downgrade(self);
        v.connect_drag_drop(move |v, ctx, x, y, time| {
            match v.drag_dest_find_target(ctx, None) {
                // Out of an archive: the archive manager saves the files.
                Some(t) if crate::xds::is(&t) => {
                    let dest = w.upgrade().map(|c| c.drop_target(x, y).0);
                    if !dest.is_some_and(|d| crate::xds::start(v, ctx, &d, time)) {
                        ctx.drag_finish(false, false, time);
                    }
                }
                // Out of Ark: Ark extracts them.
                Some(t) if crate::arkdnd::is(&t) => match w.upgrade() {
                    Some(c) => crate::arkdnd::start(v, ctx, &c.drop_target(x, y).0, time),
                    None => ctx.drag_finish(false, false, time),
                },
                Some(t) => v.drag_get_data(ctx, &t, time),
                None => ctx.drag_finish(false, false, time),
            }
            true
        });
        let w = Rc::downgrade(self);
        v.connect_drag_data_received(move |v, ctx, x, y, sel, _, time| {
            v.set_drag_dest_row(None, gtk::TreeViewDropPosition::IntoOrBefore);
            if crate::xds::is(&sel.target()) {
                crate::xds::finish(ctx, sel, time);
                return;
            }
            if crate::arkdnd::received(v, ctx, sel, time) {
                return;
            }
            let Some(c) = w.upgrade() else { return };
            let uris: Vec<String> = sel.uris().iter().map(|s| s.to_string()).collect();
            let (dest, _) = c.drop_target(x, y);
            let action = crate::pane::choose_action(v.upcast_ref(), ctx, &dest);
            ctx.drag_finish(!uris.is_empty(), false, time);
            if !uris.is_empty() {
                (hooks.drop)(uris, dest, action);
            }
        });
    }

    /// Where a drop at (`x`, `y`) (widget coordinates) goes: the folder row
    /// there (also returned, to highlight it), or else the column's folder.
    fn drop_target(&self, x: i32, y: i32) -> (gio::File, Option<gtk::TreePath>) {
        if let Some((Some(path), _)) = self.view.dest_row_at_pos(x, y) {
            if let Some(it) = self.store.iter(&path) {
                if self.store.gv::<bool>(&it, C_IS_DIR as i32) {
                    return (gio::File::for_uri(&self.store.gv::<String>(&it, C_URI as i32)), Some(path));
                }
            }
        }
        (self.folder.clone(), None)
    }

    /// Shows `uri` as selected (the folder on the way to the open one).
    pub fn highlight(&self, uri: Option<&str>) {
        *self.highlight.borrow_mut() = uri.map(str::to_string);
        if self.loaded.get() {
            self.apply_highlight();
        }
    }

    fn apply_highlight(&self) {
        let sel = self.view.selection();
        sel.unselect_all();
        let Some(want) = self.highlight.borrow().clone() else { return };
        let found = self.store.rows().into_iter().find(|it| self.store.gv::<String>(it, C_URI as i32) == want);
        if let Some(p) = found.and_then(|it| self.store.path(&it)) {
            sel.select_path(&p);
            crate::util::scroll_tree_to(&self.view, &p, false, 0.0, 0.0);
        }
    }

    fn queue_reload(self: &Rc<Self>) {
        if self.reload_queued.replace(true) {
            return;
        }
        let w = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(200), move || {
            if let Some(c) = w.upgrade() {
                c.reload_queued.set(false);
                c.load();
            }
        });
    }

    fn load(self: &Rc<Self>) {
        let gen = self.gen.get() + 1;
        self.gen.set(gen);
        let w = Rc::downgrade(self);
        let folder = self.folder.clone();
        let show_hidden = self.show_hidden;
        glib::MainContext::default().spawn_local(async move {
            // No metadata (emblems, custom icons): reading it can wait on
            // GVfs's metadata service (see metadata.rs).
            let attrs = "standard::name,standard::display-name,standard::type,standard::icon,standard::is-hidden,standard::is-backup";
            let Ok(en) = folder.enumerate_children_future(attrs, gio::FileQueryInfoFlags::NONE, glib::Priority::DEFAULT).await else { return };
            let mut rows = vec![];
            loop {
                let Ok(infos) = en.next_files_future(200, glib::Priority::DEFAULT).await else { break };
                if infos.is_empty() {
                    break;
                }
                for info in infos {
                    if !show_hidden && (util::info_is_hidden(&info) || util::info_is_backup(&info)) {
                        continue;
                    }
                    let name = info.display_name().to_string();
                    let is_dir = matches!(info.file_type(), gio::FileType::Directory | gio::FileType::Mountable);
                    let uri = folder.child(info.name()).uri().to_string();
                    let icon = crate::icons::item_icon(&info, &[]).unwrap_or_else(|| gio::ThemedIcon::new("text-x-generic").upcast());
                    rows.push((icon, name, uri, is_dir));
                }
            }
            let Some(c) = w.upgrade() else { return };
            if c.gen.get() != gen {
                return;
            }
            // Filled in one go, so a reload doesn't flicker.
            c.store.clear();
            for (icon, name, uri, is_dir) in rows {
                c.store.insert_with_values(
                    None,
                    &[(C_ICON, &icon), (C_NAME, &name), (C_URI, &uri), (C_IS_DIR, &is_dir), (C_KEY, &crate::pane::collate_key(&name))],
                );
            }
            c.loaded.set(true);
            c.apply_highlight();
        });
    }
}
