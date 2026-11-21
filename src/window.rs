//! Main window: menubar, toolbar, location bar, sidebar, tabs, status bar,
//! and all the commands (actions) that operate on the current tab.

use gio::prelude::*;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::actions;
use crate::i18n::{fill, n_, tr, trn};
use crate::bulkrename;
use crate::config;
use crate::connect;
use crate::folders;
use crate::keys;
use crate::thumbnailers;
use crate::icons;
use crate::ops::{self, Item, Job, Kind};
use crate::pane::{Event, ModelGet, Pane, ViewMode};
use crate::{clipboard, props, util};

mod menus;
mod prefs;

thread_local! {
    static WINDOWS: RefCell<Vec<Rc<AppWindow>>> = const { RefCell::new(Vec::new()) };
}

pub fn all_windows() -> Vec<Rc<AppWindow>> {
    WINDOWS.with(|w| w.borrow().clone())
}

/// Make every window's menus and toolbar match its current tab again.
fn sync_all_windows() {
    for w in all_windows() {
        if let Some(p) = w.current_pane() {
            w.update_for_pane(&p);
        }
    }
}

/// After the default sorting or the per-folder option changed: re-apply each
/// tab's folder settings, or the global sorting when per-folder views are off.
fn reapply_view_settings() {
    let s = config::get();
    for_all_panes(|p| {
        if s.per_folder {
            p.apply_folder_settings();
        } else {
            p.apply_sort(&s.sort_by, s.sort_desc);
        }
        p.reload_if_details_missing();
    });
    sync_all_windows();
}

/// The window that holds `pane` now (tabs can move between windows).
const BAR_TARGET: &str = "ARKHA_BAR_GROUP";

thread_local! {
    /// The buttons that drag their bar group (see enable_bar_drag).
    static BAR_DRAGGERS: RefCell<Vec<glib::WeakRef<gtk::Widget>>> = const { RefCell::new(Vec::new()) };
}

/// Lets `button` start a drag of a bar group, unless the bar is locked
/// (View → Lock Toolbar).
fn set_bar_draggable(button: &gtk::Widget, locked: bool) {
    if locked {
        button.drag_source_unset();
    } else {
        let targets = [gtk::TargetEntry::new(BAR_TARGET, gtk::TargetFlags::SAME_APP, 0)];
        button.drag_source_set(gdk::ModifierType::BUTTON1_MASK, &targets, gdk::DragAction::MOVE);
    }
}

/// Locks or unlocks the bar in every window.
fn lock_bars(locked: bool) {
    BAR_DRAGGERS.with(|d| {
        for b in d.borrow().iter().filter_map(|w| w.upgrade()) {
            set_bar_draggable(&b, locked);
        }
    });
}

/// Lets `button`, part of location bar group `id`, be dragged onto another
/// group to move the whole group in front of or behind it.
fn enable_bar_drag(button: &gtk::Widget, id: &'static str, group: &gtk::Widget) {
    set_bar_draggable(button, config::with(|s| s.lock_bar));
    BAR_DRAGGERS.with(|d| {
        let mut d = d.borrow_mut();
        // Drop the buttons of closed windows.
        d.retain(|w| w.upgrade().is_some());
        d.push(button.downgrade());
    });
    {
        let group = group.clone();
        button.connect_drag_begin(move |_, ctx| {
            // For tools/lockbar-test.sh (G_MESSAGES_DEBUG=arkha).
            glib::g_debug!("arkha", "bar drag started: {id}");
            // Drag a picture of the whole group.
            let (w, h) = (group.allocated_width(), group.allocated_height());
            if let Ok(surface) = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w, h) {
                if let Ok(cr) = gtk::cairo::Context::new(&surface) {
                    group.draw(&cr);
                }
                surface.set_device_offset(-(w as f64) / 2.0, -(h as f64) / 2.0);
                ctx.drag_set_icon_surface(&surface);
            }
        });
    }
    // A custom target, so the id goes as raw bytes (set_text is only for
    // text targets).
    button.connect_drag_data_get(move |_, _, data, _, _| {
        data.set(&data.target(), 8, id.as_bytes());
    });
    enable_bar_drop(button, id, group);
}

/// Lets a dragged button group be dropped on `widget`, which is part of
/// location bar item `id` (`item` being the whole item).
fn enable_bar_drop(widget: &gtk::Widget, id: &'static str, item: &gtk::Widget) {
    let targets = [gtk::TargetEntry::new(BAR_TARGET, gtk::TargetFlags::SAME_APP, 0)];
    widget.drag_dest_set(gtk::DestDefaults::ALL, &targets, gdk::DragAction::MOVE);
    let group = item.clone();
    widget.connect_drag_data_received(move |b, _, x, _, data, _, _| {
        let Ok(src) = String::from_utf8(data.data()) else { return };
        if src == id || !config::BAR_GROUPS.contains(&src.as_str()) {
            return;
        }
        // In front of this group or behind it, depending on the half hit.
        let after = b.translate_coordinates(&group, x, 0).is_some_and(|(gx, _)| gx > group.allocated_width() / 2);
        config::update(|s| {
            s.bar_order.retain(|g| *g != src);
            let i = s.bar_order.iter().position(|g| g == id).unwrap_or(0);
            s.bar_order.insert(if after { i + 1 } else { i }, src.clone());
        });
        for w in all_windows() {
            w.apply_bar_order();
        }
    });
}

/// The notebook (pane of a window) a tab's page is in.
fn notebook_of(page: &gtk::Widget) -> Option<gtk::Notebook> {
    page.parent()?.downcast().ok()
}

/// Where a new tab goes in its tab bar (None: at the end), with new tabs
/// going `mode`. `from_current`: opened from the current tab (`current`);
/// `last_opened`: the last tab opened from it since it was chosen.
///
/// "in-order" is Firefox's way: a tab opened from the current one goes
/// right after it, and each next one after the one before, so they stay in
/// the order opened; once another tab is chosen, it starts over from that
/// one. Other new tabs (Ctrl+T) go to the end.
fn tab_index(mode: &str, from_current: bool, current: Option<u32>, last_opened: Option<u32>) -> Option<u32> {
    match mode {
        "end" => None,
        "after" => current.map(|c| c + 1),
        _ if from_current => current.map(|c| last_opened.filter(|&l| l > c).unwrap_or(c) + 1),
        _ => None,
    }
}

fn owner_of(pane: &Rc<Pane>) -> Option<Rc<AppWindow>> {
    all_windows().into_iter().find(|w| w.panes.borrow().iter().any(|p| Rc::ptr_eq(p, pane)))
}

/// The list view's columns to choose and put in order (Preferences, and
/// View → Visible Columns): each detail with a check box, those shown first
/// and in their order. Changes show at once.
fn column_chooser() -> gtk::Box {
    let outer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    let sw = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    sw.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    sw.set_shadow_type(gtk::ShadowType::In);
    // All of them without scrolling.
    sw.set_min_content_height(370);
    sw.set_hexpand(true);
    sw.add(&list);
    let buttons = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let up = gtk::Button::with_mnemonic(&tr("Move _Up"));
    let down = gtk::Button::with_mnemonic(&tr("Move _Down"));
    let reset = gtk::Button::with_mnemonic(&tr("_Reset to Default"));
    for b in [&up, &down, &reset] {
        buttons.add(b);
    }
    outer.pack_start(&sw, true, true, 0);
    outer.pack_start(&buttons, false, false, 0);

    // The setting: the checked rows' details, in the list's order.
    let save = {
        let list = list.clone();
        Rc::new(move || {
            let ids: Vec<String> = list
                .children()
                .iter()
                .filter_map(|row| {
                    let check = row.downcast_ref::<gtk::ListBoxRow>()?.child()?.downcast::<gtk::CheckButton>().ok()?;
                    check.is_active().then(|| row.widget_name().to_string())
                })
                .collect();
            config::update(|s| s.list_columns = ids);
            show_chosen_columns();
        })
    };
    let fill: Rc<dyn Fn()> = {
        let (list, save) = (list.clone(), save.clone());
        Rc::new(move || {
            for c in list.children() {
                list.remove(&c);
            }
            let chosen = config::with(|s| crate::details::known(&s.list_columns));
            let rest = crate::details::choosable().map(|d| d.id).filter(|id| !chosen.contains(id));
            for id in chosen.iter().copied().chain(rest.collect::<Vec<_>>()) {
                let Some(d) = crate::details::get(id) else { continue };
                let row = gtk::ListBoxRow::new();
                row.set_widget_name(d.id);
                let check = gtk::CheckButton::with_label(&tr(d.title));
                check.set_active(chosen.contains(&d.id));
                check.set_margin_start(6);
                let save = save.clone();
                check.connect_toggled(move |_| save());
                row.add(&check);
                list.add(&row);
            }
            list.show_all();
        })
    };
    fill();
    let shift = |by: i32| {
        let (list, save) = (list.clone(), save.clone());
        move |_: &gtk::Button| {
            let Some(row) = list.selected_row() else { return };
            let to = row.index() + by;
            if to < 0 || to >= list.children().len() as i32 {
                return;
            }
            list.remove(&row);
            list.insert(&row, to);
            list.select_row(Some(&row));
            save();
        }
    };
    up.connect_clicked(shift(-1));
    down.connect_clicked(shift(1));
    reset.connect_clicked(move |_| {
        config::update(|s| s.list_columns = crate::details::DEFAULT_COLUMNS.iter().map(|c| c.to_string()).collect());
        fill();
        show_chosen_columns();
    });
    outer
}

/// Every pane's list view with the columns now chosen, its folder read
/// again if one of them needs more than it was read with.
fn show_chosen_columns() {
    for_all_panes(|p| {
        p.apply_column_choice();
        p.reload_if_details_missing();
    });
}

/// Reads `files` again wherever they're shown (their metadata changed:
/// GVfs doesn't tell folder monitors about that).
/// Shows `folder`'s background again wherever it's open (it changed).
pub fn show_backgrounds_of(folder: &gio::File) {
    for_all_panes(|p| {
        if !p.is_searching() && p.current().equal(folder) {
            p.show_background();
        }
    });
}

/// After the setting for operations in their own window changed: the
/// operations button shown or hidden, and the window opened if operations
/// are running.
pub fn ops_shown_as_set() {
    for w in all_windows() {
        w.update_ops_button();
    }
    if config::with(|s| s.ops_window) && ops::is_busy() {
        ops::open_window();
    }
}

/// Every window's side pane with the background now chosen.
pub fn show_side_pane_backgrounds() {
    for w in all_windows() {
        w.sidebar.show_background();
    }
}

pub fn refresh_everywhere(files: &[gio::File]) {
    for_all_panes(|p| {
        let shown: Vec<gio::File> = files.iter().filter(|f| p.shows(&f.uri())).cloned().collect();
        if !shown.is_empty() {
            p.refresh_metadata(&shown);
        }
    });
}

fn for_all_panes(f: impl Fn(&Rc<Pane>)) {
    for w in all_windows() {
        for p in w.panes.borrow().iter() {
            f(p);
        }
    }
}

/// What Reopen Closed Tab brings back.
struct ClosedTab {
    place: crate::pane::Place,
    back: Vec<crate::pane::Place>,
    fwd: Vec<crate::pane::Place>,
    position: u32,
}

const MAX_CLOSED_TABS: usize = 25;

pub struct AppWindow {
    pub window: gtk::ApplicationWindow,
    notebook: gtk::Notebook,
    /// Split view's extra pane, shown only while split view is on.
    notebook2: gtk::Notebook,
    split: gtk::Paned,
    /// The pane (notebook) the toolbar, menus and shortcuts act on.
    active_nb: RefCell<gtk::Notebook>,
    /// Set while tabs are moved between our own panes, so the brief moment
    /// with no tabs isn't mistaken for the window being emptied.
    moving_tabs: Cell<bool>,
    sidebar: Rc<crate::sidebar::Sidebar>,
    paned: gtk::Paned,
    back_btn: gtk::Button,
    toolbar_labels: Vec<gtk::Label>,
    path_area: gtk::Box,
    zoom_box: gtk::Box,
    view_box: gtk::Box,
    /// The split button and, in split view, the Swap Panes button.
    split_box: gtk::Box,
    swap_btn: gtk::Button,
    info_btn: gtk::ToggleButton,
    ops_btn: gtk::ToggleButton,
    ops_bar: gtk::ProgressBar,
    ops_pop: gtk::Popover,
    path_box: gtk::Box,
    path_scroll: gtk::ScrolledWindow,
    crumbs: RefCell<Vec<gio::File>>,
    location_entry: gtk::Entry,
    loc_stack: gtk::Stack,
    search_field: gtk::Entry,
    /// The pane and folder the search bar was opened for (Ctrl+F, the
    /// toolbar's search button) while it's shown without results: as in
    /// Caja it goes away when that pane goes elsewhere.
    search_pane: RefCell<Option<(std::rc::Weak<Pane>, String)>>,
    /// The toolbar's search button: pressed while the search bar shows.
    search_btn: gtk::ToggleButton,
    /// Set while the search widgets are updated to match a pane, so their
    /// handlers leave the pane alone.
    search_syncing: Cell<bool>,
    loc_toggle: gtk::ToggleButton,
    status: gtk::Label,
    statusbar: gtk::Box,
    toolbar: gtk::Box,
    locbar: gtk::Box,
    spinner: gtk::Spinner,
    panes: RefCell<Vec<Rc<Pane>>>,
    /// Recently closed tabs, most recent last, for Reopen Closed Tab.
    closed_tabs: RefCell<Vec<ClosedTab>>,
    /// The last tab opened from the current one since it was chosen (see
    /// tab_index): the next one opened from it goes after this one.
    last_opened: RefCell<Option<glib::WeakRef<gtk::Widget>>>,
    bookmarks_menu: gio::Menu,
    templates_menu: gio::Menu,
    /// The Edit menu's Undo and Redo items, labelled with what they'd do.
    undo_menu: gio::Menu,
    undo_label: RefCell<Option<(Option<String>, Option<String>)>>,
    /// The folders in a folder (path, names), read for completing the
    /// location typed.
    completion_dir: RefCell<Option<(std::path::PathBuf, Vec<String>)>>,
    /// The location entry's text is being set by us: no completion.
    completing: Cell<bool>,
    _bookmark_monitor: RefCell<Option<gio::FileMonitor>>,
}

/// Where the extra pane of split view opens, by Preferences (`opens`, see
/// Settings::split_opens): the folder of the pane in use (`here`), home,
/// where the closed pane was (`last`) or the chosen `folder`; `here` when
/// those aren't set, or a local one is gone.
fn split_opens_at(opens: &str, here: &gio::File, folder: &str, last: &str) -> gio::File {
    let usable = |uri: &str| {
        let f = gio::File::for_uri(uri);
        (!uri.is_empty() && (!f.is_native() || f.query_exists(gio::Cancellable::NONE))).then_some(f)
    };
    match opens {
        "home" => util::home_file(),
        "last" => usable(last).unwrap_or_else(|| here.clone()),
        "folder" => usable(folder).unwrap_or_else(|| here.clone()),
        _ => here.clone(),
    }
}

impl AppWindow {
    pub fn new(app: &gtk::Application, locations: &[gio::File]) -> Rc<AppWindow> {
        Self::create(app, locations, true)
    }

    /// `home_if_empty`: open the home folder when no locations are given;
    /// otherwise the window starts without tabs (to receive a dragged one).
    fn create(app: &gtk::Application, locations: &[gio::File], home_if_empty: bool) -> Rc<AppWindow> {
        let s = config::get();
        let window = gtk::ApplicationWindow::new(app);
        window.set_default_size(s.width, s.height);
        if s.maximized {
            window.maximize();
        }
        window.set_icon_name(Some(util::app_icon()));

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window.add(&vbox);

        // ---------------- toolbar
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        toolbar.set_border_width(3);
        toolbar.style_context().add_class("primary-toolbar");
        // Button texts, kept to show or hide them (Preferences → Display).
        let toolbar_labels: RefCell<Vec<gtk::Label>> = RefCell::new(vec![]);
        let tbtn = |icon: &str, tip: &str, action: &str, label: Option<&str>| {
            let b = gtk::Button::new();
            let bx = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            bx.pack_start(&gtk::Image::from_icon_name(Some(icon), gtk::IconSize::LargeToolbar), false, false, 0);
            if let Some(l) = label {
                let l = gtk::Label::new(Some(l));
                // Not shown by show_all(); its visibility follows the setting.
                l.set_no_show_all(true);
                l.set_visible(s.toolbar_text);
                bx.pack_start(&l, false, false, 0);
                toolbar_labels.borrow_mut().push(l);
            }
            b.add(&bx);
            b.set_relief(gtk::ReliefStyle::None);
            b.set_tooltip_text(Some(tip));
            b.set_action_name(Some(action));
            b.set_focus_on_click(false);
            b
        };
        let back_btn = tbtn("go-previous", &tr("Go to the previous visited location (right-click for history)"), "win.back", Some(&tr("Back")));
        let fwd_btn = tbtn("go-next", &tr("Go to the next visited location"), "win.forward", Some(&tr("Forward")));
        let up_btn = tbtn("go-up", &tr("Open the parent folder"), "win.up", None);
        let reload_btn = tbtn("view-refresh", &tr("Reload the current location"), "win.reload", None);
        let home_btn = tbtn("go-home", &tr("Open your personal folder"), "win.home", None);
        let root_btn = tbtn("drive-harddisk", &tr("Browse the file system"), "win.go-root", None);
        // Caja's Search for Files button: shows and hides the search bar.
        let search_btn = gtk::ToggleButton::new();
        search_btn.add(&gtk::Image::from_icon_name(Some("edit-find"), gtk::IconSize::LargeToolbar));
        search_btn.set_relief(gtk::ReliefStyle::None);
        search_btn.set_tooltip_text(Some(&tr("Search documents and folders by name")));
        search_btn.set_focus_on_click(false);
        for b in [&back_btn, &fwd_btn, &up_btn] {
            toolbar.pack_start(b, false, false, 0);
        }
        toolbar.pack_start(&gtk::Separator::new(gtk::Orientation::Vertical), false, false, 4);
        for b in [&reload_btn, &home_btn, &root_btn] {
            toolbar.pack_start(b, false, false, 0);
        }
        toolbar.pack_start(&gtk::Separator::new(gtk::Orientation::Vertical), false, false, 4);
        toolbar.pack_start(&search_btn, false, false, 0);
        let spinner = gtk::Spinner::new();
        toolbar.pack_end(&spinner, false, false, 6);

        // ---------------- location bar
        let locbar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        locbar.set_border_width(3);
        let loc_toggle = gtk::ToggleButton::new();
        loc_toggle.add(&gtk::Image::from_icon_name(Some("document-edit-symbolic"), gtk::IconSize::Button));
        loc_toggle.set_tooltip_text(Some(&tr("Toggle between button and text-based location bar (Ctrl+L)")));
        loc_toggle.set_focus_on_click(false);
        // The path (edit toggle + breadcrumbs or text) is one item that the
        // button groups can be dragged to either side of.
        let path_area = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        path_area.set_hexpand(true);
        path_area.pack_start(&loc_toggle, false, false, 0);
        locbar.pack_start(&path_area, true, true, 0);
        // Separate buttons 3 px apart, as Caja's path bar.
        let path_box = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        let path_scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        path_scroll.set_policy(gtk::PolicyType::External, gtk::PolicyType::Never);
        path_scroll.add(&path_box);
        path_scroll.set_hexpand(true);
        let location_entry = gtk::Entry::new();
        location_entry.set_hexpand(true);
        location_entry.set_placeholder_text(Some(&tr("Type a path, e.g. ~/Documents or /etc")));
        // Ctrl+F: a search field in place of the path, as in Caja (Enter
        // searches; see the search bar's signals).
        let search_field = gtk::Entry::new();
        search_field.set_hexpand(true);
        search_field.set_icon_from_icon_name(gtk::EntryIconPosition::Secondary, Some("edit-find-symbolic"));
        let search_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let search_label = gtk::Label::with_mnemonic(&tr("_Search:"));
        search_label.set_mnemonic_widget(Some(&search_field));
        search_label.set_margin_start(6);
        search_box.pack_start(&search_label, false, false, 0);
        search_box.pack_start(&search_field, true, true, 0);
        let loc_stack = gtk::Stack::new();
        loc_stack.add_named(&path_scroll, "path");
        loc_stack.add_named(&location_entry, "entry");
        loc_stack.add_named(&search_box, "search");
        loc_stack.set_hexpand(true);
        path_area.pack_start(&loc_stack, true, true, 0);
        // zoom + view switcher
        let zoom_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        zoom_box.style_context().add_class("linked");
        for (icon, act, tip) in [
            ("zoom-out-symbolic", "win.zoom-out", n_("Zoom out")),
            ("zoom-original-symbolic", "win.zoom-normal", n_("Normal size")),
            ("zoom-in-symbolic", "win.zoom-in", n_("Zoom in")),
        ] {
            let b = gtk::Button::from_icon_name(Some(icon), gtk::IconSize::Button);
            b.set_action_name(Some(act));
            b.set_tooltip_text(Some(&tr(tip)));
            b.set_focus_on_click(false);
            zoom_box.pack_start(&b, false, false, 0);
        }
        locbar.pack_start(&zoom_box, false, false, 3);
        let view_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        view_box.style_context().add_class("linked");
        for (icon, target, tip) in [
            ("view-grid-symbolic", "icon", n_("Icon View (Ctrl+1)")),
            ("view-list-symbolic", "list", n_("List View (Ctrl+2)")),
            ("view-continuous-symbolic", "compact", n_("Compact View (Ctrl+3)")),
            ("view-column-symbolic", "columns", n_("Columns View (Ctrl+4)")),
        ] {
            let b = gtk::ToggleButton::new();
            // view-column isn't in every theme (Adwaita lacks it).
            let names = [icon, "view-paged-symbolic"];
            b.add(&gtk::Image::from_gicon(&gio::ThemedIcon::from_names(&names), gtk::IconSize::Button));
            b.set_action_name(Some("win.view-mode"));
            b.set_action_target_value(Some(&target.to_variant()));
            b.set_tooltip_text(Some(tip));
            b.set_focus_on_click(false);
            // Clicking the pressed one would release it: the view doesn't
            // change, so GTK doesn't press it again.
            let win = window.downgrade();
            b.connect_toggled(move |b| {
                let current = win
                    .upgrade()
                    .and_then(|w| w.lookup_action("view-mode"))
                    .and_then(|a| a.state())
                    .and_then(|v| v.get::<String>());
                if !b.is_active() && current.as_deref() == Some(target) {
                    b.set_active(true);
                }
            });
            view_box.pack_start(&b, false, false, 0);
        }
        locbar.pack_start(&view_box, false, false, 3);
        let split_btn = gtk::ToggleButton::new();
        split_btn.add(&gtk::Image::from_icon_name(Some("view-dual-symbolic"), gtk::IconSize::Button));
        split_btn.set_action_name(Some("win.split-view"));
        split_btn.set_tooltip_text(Some(&tr("Split view: show a second pane")));
        split_btn.set_focus_on_click(false);
        let swap_btn = gtk::Button::new();
        let swap_icon = gio::ThemedIcon::from_names(&["object-flip-horizontal-symbolic", "view-dual-symbolic"]);
        swap_btn.add(&gtk::Image::from_gicon(&swap_icon, gtk::IconSize::Button));
        swap_btn.set_action_name(Some("win.swap-panes"));
        swap_btn.set_tooltip_text(Some(&tr("Swap the panes: their tabs change sides (Ctrl+U)")));
        swap_btn.set_focus_on_click(false);
        // An icon without words: named for screen readers.
        if let Some(a) = swap_btn.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&a, &tr("Swap Panes"));
        }
        let split_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        split_box.style_context().add_class("linked");
        split_box.pack_start(&split_btn, false, false, 0);
        split_box.pack_start(&swap_btn, false, false, 0);
        locbar.pack_start(&split_box, false, false, 3);
        let info_btn = gtk::ToggleButton::new();
        // A window with a panel on its right; fallbacks for themes without it.
        let info_icon = gio::ThemedIcon::from_names(&["sidebar-show-right-symbolic", "view-right-pane-symbolic", "dialog-information-symbolic"]);
        info_btn.add(&gtk::Image::from_gicon(&info_icon, gtk::IconSize::Button));
        info_btn.set_action_name(Some("win.info-panel"));
        info_btn.set_tooltip_text(Some(&tr("Info panel: the selected item's picture and details, in this folder (F11)")));
        info_btn.set_focus_on_click(false);
        locbar.pack_start(&info_btn, false, false, 3);
        // File operations, like a browser's downloads button: shown while
        // there are any, with their overall progress; click for the list.
        let ops_btn = gtk::ToggleButton::new();
        ops_btn.set_tooltip_text(Some(&tr("File operations")));
        ops_btn.set_focus_on_click(false);
        let ops_inner = gtk::Box::new(gtk::Orientation::Vertical, 1);
        ops_inner.add(&gtk::Image::from_icon_name(Some("edit-copy-symbolic"), gtk::IconSize::Button));
        let ops_bar = gtk::ProgressBar::new();
        let bar_css = gtk::CssProvider::new();
        let _ = bar_css.load_from_data(b"progressbar trough, progressbar progress { min-height: 2px; } progressbar trough { min-width: 16px; }");
        ops_bar.style_context().add_provider(&bar_css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        ops_inner.add(&ops_bar);
        ops_btn.add(&ops_inner);
        locbar.pack_start(&ops_btn, false, false, 3);
        let ops_pop = gtk::Popover::new(Some(&ops_btn));
        // This window's own list of the operations (it keeps itself current).
        let ops_slot = ops::new_view();
        ops_slot.set_size_request(420, -1);
        let ops_scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        ops_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        ops_scroll.set_propagate_natural_height(true);
        ops_scroll.set_max_content_height(420);
        ops_scroll.add(&ops_slot);
        let open_win = gtk::Button::with_label(&tr("Open in Window"));
        open_win.set_relief(gtk::ReliefStyle::None);
        open_win.set_margin_top(4);
        open_win.set_margin_bottom(4);
        let pop_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        pop_box.add(&ops_scroll);
        pop_box.add(&gtk::Separator::new(gtk::Orientation::Horizontal));
        pop_box.add(&open_win);
        pop_box.show_all();
        ops_pop.add(&pop_box);
        {
            let pop = ops_pop.clone();
            ops_btn.connect_toggled(move |b| {
                if b.is_active() {
                    pop.popup();
                } else {
                    pop.popdown();
                }
            });
        }
        {
            let btn = ops_btn.clone();
            ops_pop.connect_closed(move |_| btn.set_active(false));
        }
        {
            let pop = ops_pop.clone();
            open_win.connect_clicked(move |_| {
                pop.popdown();
                ops::open_window();
            });
        }
        ops_btn.show_all();
        ops_btn.set_no_show_all(true);
        ops_btn.set_visible(false);
        // Each group can be hidden in Preferences; show_all() leaves them be.
        for (w, on) in [(zoom_box.upcast_ref::<gtk::Widget>(), s.show_zoom_buttons), (view_box.upcast_ref(), s.show_view_buttons), (split_box.upcast_ref(), s.show_split_button), (info_btn.upcast_ref(), s.show_info_button)] {
            w.show_all();
            w.set_no_show_all(true);
            w.set_visible(on);
        }
        // Only while there are two panes to swap (see set_split).
        swap_btn.set_no_show_all(true);
        swap_btn.set_visible(false);
        // Any group can be dragged onto another, or onto the path, to
        // rearrange the bar.
        enable_bar_drop(path_area.upcast_ref(), "path", path_area.upcast_ref());
        for (id, group) in [("zoom", zoom_box.clone().upcast::<gtk::Widget>()), ("view", view_box.clone().upcast()), ("split", split_box.clone().upcast()), ("info", info_btn.clone().upcast()), ("ops", ops_btn.clone().upcast())] {
            let buttons = match group.downcast_ref::<gtk::Box>() {
                Some(b) => b.children(),
                None => vec![group.clone()],
            };
            for b in &buttons {
                enable_bar_drag(b, id, &group);
            }
        }

        // ---------------- body
        let sidebar = crate::sidebar::Sidebar::new();
        apply_sidebar_css();
        // Themes like Adwaita put a margin around each tab; make the tabs
        // touch. In split view the pane without focus is dimmed.
        let css = gtk::CssProvider::new();
        let _ = css.load_from_data(b"notebook > header tabs { padding-left: 0; padding-right: 0; } notebook > header tabs tab { margin-left: 0; margin-right: 0; } notebook.arkha-inactive { opacity: 0.7; }");
        let new_notebook = || {
            let nb = gtk::Notebook::new();
            nb.set_scrollable(true);
            nb.set_show_border(false);
            nb.style_context().add_provider(&css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
            // Shared group: tabs can be dragged between windows (see
            // adopt_page) and panes, or out of a window into a new one.
            nb.set_group_name(Some("arkha-tabs"));
            nb
        };
        let notebook = new_notebook();
        // The extra pane of split view (F3), hidden until it's turned on.
        let notebook2 = new_notebook();
        notebook2.set_no_show_all(true);
        let split = gtk::Paned::new(gtk::Orientation::Horizontal);
        split.pack1(&notebook, true, false);
        split.pack2(&notebook2, true, false);
        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        paned.pack1(&sidebar.widget, false, false);
        paned.pack2(&split, true, false);
        paned.set_position(s.sidebar_width);

        // ---------------- status bar
        let statusbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        statusbar.set_border_width(3);
        let status = gtk::Label::new(None);
        status.set_xalign(0.0);
        status.set_ellipsize(pango::EllipsizeMode::End);
        status.set_margin_start(6);
        statusbar.pack_start(&status, true, true, 0);

        let menubar_model = gio::Menu::new();
        let bookmarks_menu = gio::Menu::new();
        let templates_menu = gio::Menu::new();

        let this = Rc::new(AppWindow {
            window: window.clone(),
            active_nb: RefCell::new(notebook.clone()),
            moving_tabs: Cell::new(false),
            notebook,
            notebook2,
            split,
            sidebar,
            paned: paned.clone(),
            back_btn,
            toolbar_labels: toolbar_labels.into_inner(),
            path_area,
            zoom_box,
            view_box,
            split_box,
            swap_btn,
            info_btn,
            ops_btn,
            ops_bar,
            ops_pop,
            path_box,
            path_scroll,
            crumbs: RefCell::new(vec![]),
            location_entry,
            loc_stack,
            search_field,
            search_pane: RefCell::new(None),
            search_btn,
            search_syncing: Cell::new(false),
            loc_toggle,
            status,
            statusbar: statusbar.clone(),
            toolbar: toolbar.clone(),
            locbar: locbar.clone(),
            spinner,
            panes: RefCell::new(vec![]),
            closed_tabs: RefCell::new(vec![]),
            last_opened: RefCell::new(None),
            bookmarks_menu,
            templates_menu,
            undo_menu: gio::Menu::new(),
            // Not labels update_undo() would give: the first call sets them.
            undo_label: RefCell::new(None),
            completion_dir: RefCell::new(None),
            completing: Cell::new(false),
            _bookmark_monitor: RefCell::new(None),
        });

        this.build_menubar(&menubar_model);
        let menubar = gtk::MenuBar::from_model(&menubar_model);
        vbox.pack_start(&menubar, false, false, 0);
        vbox.pack_start(&toolbar, false, false, 0);
        vbox.pack_start(&locbar, false, false, 0);
        vbox.pack_start(&gtk::Separator::new(gtk::Orientation::Horizontal), false, false, 0);
        vbox.pack_start(&paned, true, true, 0);
        vbox.pack_start(&statusbar, false, false, 0);

        this.apply_bar_order();
        {
            let w = Rc::downgrade(&this);
            let update = move || {
                if let Some(s) = w.upgrade() {
                    s.update_ops_button();
                    s.update_undo();
                }
            };
            ops::on_change(update);
        }
        {
            // Search results have no folder monitor: they learn here of
            // items moved, trashed or deleted.
            let w = Rc::downgrade(&this);
            ops::on_moved(move |went| {
                if let Some(s) = w.upgrade() {
                    for p in s.panes.borrow().iter() {
                        p.files_moved(went);
                    }
                }
            });
        }
        this.update_ops_button();
        this.setup_actions();
        this.update_undo();
        this.setup_signals();
        this.refresh_bookmarks();
        this.refresh_templates();

        let locs: Vec<gio::File> = if locations.is_empty() && home_if_empty { vec![util::home_file()] } else { locations.to_vec() };
        // Opened in the given order: when tabs go after the current one,
        // switch to each as it's added (the first is shown below).
        let after_current = s.new_tabs == "after";
        for l in &locs {
            this.add_tab(l, after_current);
        }
        if !locs.is_empty() {
            this.notebook.set_current_page(Some(0));
        }
        // Everything shown but the window itself, so that the icons it will
        // show can be loaded all at once first (see preload_widget_icons).
        // As show_all() does: foreach, not forall, whose internal children
        // (popovers) mustn't be shown, and showing them changes the list
        // being walked.
        window.foreach(|w| w.show_all());
        this.sidebar.widget.set_visible(s.show_sidebar);
        this.statusbar.set_visible(s.show_statusbar);
        this.toolbar.set_visible(s.show_toolbar);
        this.apply_path_mode();
        this.update_locbar_visibility();
        this.update_tabs_visibility();
        icons::preload_widget_icons(window.upcast_ref());
        window.show();
        WINDOWS.with(|w| w.borrow_mut().push(this.clone()));
        if let Some(p) = this.current_pane() {
            // The first tab's events came before the window was in WINDOWS
            // (so owner_of() didn't find it): catch up on its state (view
            // mode, cover panel, … in the menus).
            this.update_for_pane(&p);
            p.grab_focus();
        }
        this
    }

    // ================================================================ actions

    fn add(self: &Rc<Self>, name: &str, f: impl Fn(&Rc<AppWindow>) + 'static) {
        let a = gio::SimpleAction::new(name, None);
        let w = Rc::downgrade(self);
        a.connect_activate(move |_, _| {
            if let Some(s) = w.upgrade() {
                f(&s);
            }
        });
        self.window.add_action(&a);
    }

    fn add_str(self: &Rc<Self>, name: &str, f: impl Fn(&Rc<AppWindow>, String) + 'static) {
        let a = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
        let w = Rc::downgrade(self);
        a.connect_activate(move |_, p| {
            if let (Some(s), Some(p)) = (w.upgrade(), p.and_then(|p| p.get::<String>())) {
                f(&s, p);
            }
        });
        self.window.add_action(&a);
    }

    fn add_toggle(self: &Rc<Self>, name: &str, init: bool, f: impl Fn(&Rc<AppWindow>, bool) + 'static) {
        let a = gio::SimpleAction::new_stateful(name, None, &init.to_variant());
        let w = Rc::downgrade(self);
        a.connect_activate(move |a, _| {
            let v = !a.state().and_then(|s| s.get::<bool>()).unwrap_or(false);
            a.set_state(&v.to_variant());
            if let Some(s) = w.upgrade() {
                f(&s, v);
            }
        });
        self.window.add_action(&a);
    }

    fn add_radio(self: &Rc<Self>, name: &str, init: &str, f: impl Fn(&Rc<AppWindow>, String) + 'static) {
        let a = gio::SimpleAction::new_stateful(name, Some(glib::VariantTy::STRING), &init.to_variant());
        let w = Rc::downgrade(self);
        a.connect_activate(move |a, p| {
            let Some(v) = p.and_then(|p| p.get::<String>()) else { return };
            a.set_state(&v.to_variant());
            if let Some(s) = w.upgrade() {
                f(&s, v);
            }
        });
        self.window.add_action(&a);
    }

    fn action(&self, name: &str) -> Option<gio::SimpleAction> {
        self.window
            .lookup_action(name)
            .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
    }

    /// Labels Edit → Undo and Redo with what they'd do, greyed out with nothing.
    fn update_undo(&self) {
        let labels = (ops::undo_label(), ops::redo_label());
        if self.undo_label.borrow().as_ref() == Some(&labels) {
            return;
        }
        self.undo_menu.remove_all();
        self.undo_menu.append(Some(&labels.0.clone().unwrap_or_else(|| tr("_Undo"))), Some("win.undo"));
        self.undo_menu.append(Some(&labels.1.clone().unwrap_or_else(|| tr("_Redo"))), Some("win.redo"));
        self.set_enabled("undo", labels.0.is_some());
        self.set_enabled("redo", labels.1.is_some());
        *self.undo_label.borrow_mut() = Some(labels);
    }

    fn set_enabled(&self, name: &str, on: bool) {
        if let Some(a) = self.action(name) {
            a.set_enabled(on);
        }
    }

    fn set_state(&self, name: &str, v: glib::Variant) {
        if let Some(a) = self.action(name) {
            a.set_state(&v);
        }
    }

    fn setup_actions(self: &Rc<Self>) {
        let s = config::get();
        self.add("new-tab", |w| {
            let f = w.current_pane().map(|p| p.current()).unwrap_or_else(util::home_file);
            w.add_tab(&f, true);
        });
        self.add("new-window", |w| {
            let f = w.current_pane().map(|p| p.current()).unwrap_or_else(util::home_file);
            if let Some(app) = w.window.application() {
                AppWindow::new(&app, &[f]);
            }
        });
        self.add("close-tab", |w| {
            if let Some(p) = w.current_pane() {
                w.close_tab(&p);
            }
        });
        self.add("reopen-tab", |w| w.reopen_tab());
        self.set_enabled("reopen-tab", false); // until a tab is closed
        self.add("close-window", |w| w.window.close());
        self.add("open", |w| w.open_selection(OpenMode::Normal));
        self.add("open-new-tab", |w| w.open_selection(OpenMode::Tab));
        self.add("open-new-window", |w| w.open_selection(OpenMode::Window));
        self.add_str("custom-action", |w, target| w.run_custom_action(&target));
        self.add_str("open-with", |w, id| w.open_with(&id));
        self.add("open-with-other", |w| w.open_with_other());
        self.add("new-folder", |w| w.new_folder());
        self.add("new-file", |w| w.new_file(None));
        self.add_str("new-from-template", |w, path| w.new_file(Some(path)));
        self.add("cut", |w| w.copy_selection(true));
        self.add("copy", |w| w.copy_selection(false));
        self.add("undo", |w| {
            if let Err(why) = ops::undo(Some(w.window.clone().upcast())) {
                w.flash_status(&why);
            }
        });
        self.add("redo", |w| {
            if let Err(why) = ops::redo(Some(w.window.clone().upcast())) {
                w.flash_status(&why);
            }
        });
        self.add("paste", |w| {
            if let Some(p) = w.current_pane() {
                w.paste_into(p.current());
            }
        });
        // Pasted, each copy then read back and compared with its original.
        self.add("paste-check", |w| {
            if let Some(p) = w.current_pane() {
                w.paste_with(p.current(), true);
            }
        });
        self.add("paste-into", |w| {
            if let Some(p) = w.current_pane() {
                if let Some(r) = p.selected().into_iter().find(|r| r.is_dir) {
                    w.paste_into(gio::File::for_uri(&r.uri));
                }
            }
        });
        self.add("select-all", |w| {
            if let Some(p) = w.current_pane() {
                p.select_all();
            }
        });
        self.add("invert-selection", |w| {
            if let Some(p) = w.current_pane() {
                p.invert_selection();
            }
        });
        self.add("select-pattern", |w| w.select_pattern_dialog());
        self.add("rename-all", |w| {
            if let Some(p) = w.current_pane() {
                let files = p.selected_files();
                if !files.is_empty() {
                    bulkrename::show(w.window.upcast_ref(), files);
                }
            }
        });
        self.add("rename", |w| {
            if let Some(p) = w.current_pane() {
                let sel = p.selected();
                match sel.len() {
                    0 => {}
                    1 => p.start_rename(&sel[0].uri),
                    // Several items: the Rename key opens bulk renaming.
                    _ => bulkrename::show(w.window.upcast_ref(), p.selected_files()),
                }
            }
        });
        self.add("duplicate", |w| w.transfer_selection_here(Kind::Copy));
        self.add("make-link", |w| w.transfer_selection_here(Kind::Link));
        self.add("copy-to", |w| w.transfer_to_chosen(Kind::Copy));
        self.add("move-to", |w| w.transfer_to_chosen(Kind::Move));
        self.add("copy-to-pane", |w| w.transfer_to_other_pane(Kind::Copy));
        self.add("move-to-pane", |w| w.transfer_to_other_pane(Kind::Move));
        self.add("trash", |w| w.trash_selection());
        self.add("delete", |w| w.delete_selection());
        self.add("restore", |w| w.restore_selection());
        self.add("empty-trash", |w| w.empty_trash());
        self.add("properties", |w| {
            if let Some(p) = w.current_pane() {
                let mut files = p.selected_files();
                if files.is_empty() {
                    files = vec![p.current()];
                }
                props::show(w.window.upcast_ref(), files);
            }
        });
        self.add("terminal", |w| w.open_terminal());
        self.add("extract", |w| w.archive(true));
        self.add("compress", |w| w.archive(false));
        self.add("back", |w| {
            if let Some(p) = w.current_pane() {
                p.go_back();
            }
        });
        self.add("forward", |w| {
            if let Some(p) = w.current_pane() {
                p.go_forward();
            }
        });
        self.add("up", |w| {
            if let Some(p) = w.current_pane() {
                p.go_up();
            }
        });
        self.add("home", |w| w.go(&util::home_file()));
        self.add("go-root", |w| w.go(&gio::File::for_path("/")));
        self.add("go-trash", |w| w.go(&gio::File::for_uri("trash:///")));
        self.add("go-desktop", |w| {
            let d = glib::user_special_dir(glib::UserDirectory::Desktop).unwrap_or_else(glib::home_dir);
            w.go(&gio::File::for_path(d));
        });
        self.add_str("go-uri", |w, uri| w.go(&gio::File::for_uri(&uri)));
        self.add("reload", |w| {
            if let Some(p) = w.current_pane() {
                p.reload();
            }
        });
        self.add("location", |w| w.show_location_entry(None));
        // Ctrl+F shows the search bar (as in Caja it never hides it: the
        // toolbar's button does).
        self.add("filter", |w| w.show_search());
        self.add("escape", |w| {
            w.loc_toggle.set_active(false);
            let searching = w.current_pane().is_some_and(|p| p.is_searching());
            if w.search_open() && !searching {
                w.hide_search();
            }
            if let Some(p) = w.current_pane() {
                p.grab_focus();
            }
        });
        self.add("open-parent", |w| {
            let Some(p) = w.current_pane() else { return };
            if let [r] = p.selected().as_slice() {
                p.open_parent_location(&r.uri);
            }
        });
        self.add("zoom-in", |w| {
            if let Some(p) = w.current_pane() {
                p.zoom(1);
            }
        });
        self.add("zoom-out", |w| {
            if let Some(p) = w.current_pane() {
                p.zoom(-1);
            }
        });
        self.add("zoom-normal", |w| {
            if let Some(p) = w.current_pane() {
                p.zoom_reset();
            }
        });
        self.add("add-bookmark", |w| {
            if let Some(p) = w.current_pane() {
                let uri = p.current().uri().to_string();
                match util::add_bookmark(&uri) {
                    Ok(true) => {}
                    Ok(false) => w.flash_status(&tr("This location is already bookmarked")),
                    Err(e) => util::error_dialog(Some(w.window.upcast_ref()), &tr("Could not add bookmark"), &e.to_string()),
                }
            }
        });
        self.add("edit-bookmarks", |w| w.edit_bookmarks());
        self.add("connect-server", |w| {
            let weak = Rc::downgrade(w);
            connect::show(w.window.upcast_ref(), move |uri| {
                if let Some(w) = weak.upgrade() {
                    w.go(&gio::File::for_uri(&uri));
                }
            });
        });
        // Around from the last tab to the first and back, as in browsers.
        // Move Tab Left / Right, as in Caja (Ctrl+Shift+Page Up / Down).
        for (name, step) in [("tab-left", -1i32), ("tab-right", 1)] {
            self.add(name, move |w| {
                let nb = w.active();
                let (Some(cur), n) = (nb.current_page(), nb.n_pages() as i32) else { return };
                let to = cur as i32 + step;
                if (0..n).contains(&to) {
                    if let Some(page) = nb.nth_page(Some(cur)) {
                        nb.reorder_child(&page, Some(to as u32));
                        w.update_action_states();
                    }
                }
            });
        }
        self.add("close-all-windows", |_| {
            for w in all_windows() {
                w.window.close();
            }
        });
        self.add("clear-history", |w| {
            for_all_panes(|p| p.clear_history());
            if let Some(p) = w.current_pane() {
                w.update_for_pane(&p);
            }
        });
        self.add("stop", |w| {
            if let Some(p) = w.current_pane() {
                p.stop();
            }
        });
        self.add("switch-pane", |w| {
            if w.split_on() {
                let nb = if w.active() == w.notebook { w.notebook2.clone() } else { w.notebook.clone() };
                w.set_active(&nb);
                if let Some(p) = w.current_pane() {
                    p.grab_focus();
                }
            }
        });
        self.add("same-location", |w| {
            if let (Some(p), Some(o)) = (w.current_pane(), w.other_pane()) {
                if !o.current().equal(&p.current()) {
                    p.navigate(&o.current());
                }
            }
        });
        self.add("next-tab", |w| {
            let nb = w.active();
            let n = nb.n_pages();
            if n > 0 {
                nb.set_current_page(Some((nb.current_page().unwrap_or(0) + 1) % n));
            }
        });
        self.add("prev-tab", |w| {
            let nb = w.active();
            let n = nb.n_pages();
            if n > 0 {
                nb.set_current_page(Some((nb.current_page().unwrap_or(0) + n - 1) % n));
            }
        });
        for i in 1..=9u32 {
            self.add(&format!("tab-{i}"), move |w| {
                let nb = w.active();
                let n = if i == 9 { nb.n_pages().saturating_sub(1) } else { i - 1 };
                nb.set_current_page(Some(n));
            });
        }
        self.add("preferences", |w| w.preferences());
        self.add("backgrounds", |w| crate::backgrounds::show(w.window.upcast_ref()));
        self.add("about", |w| w.about());
        self.add("shortcuts", |w| w.shortcuts());
        self.add("manual", |w| w.manual());

        self.add_toggle("show-hidden", s.show_hidden, |_, v| {
            config::update(|s| s.show_hidden = v);
            for w in all_windows() {
                w.set_state("show-hidden", v.to_variant());
            }
            for_all_panes(|p| p.set_show_hidden(v));
        });
        // The info panel, shown per folder (see Pane::set_info_panel).
        self.add_toggle("info-panel", false, |w, v| {
            if let Some(p) = w.current_pane() {
                p.set_info_panel(v);
            }
        });
        self.add_toggle("show-sidebar", s.show_sidebar, |w, v| {
            config::update(|s| s.show_sidebar = v);
            w.sidebar.widget.set_visible(v);
        });
        self.add_toggle("focus-follows-mouse", s.focus_follows_mouse, |_, v| {
            config::update(|s| s.focus_follows_mouse = v);
            // One setting: every window's switch shows it.
            for w in all_windows() {
                if let Some(a) = w.window.lookup_action("focus-follows-mouse").and_downcast::<gio::SimpleAction>() {
                    a.set_state(&v.to_variant());
                }
            }
        });
        self.add_toggle("lock-bar", s.lock_bar, |_, v| {
            config::update(|s| s.lock_bar = v);
            lock_bars(v);
            // One setting: every window's menu shows it.
            for w in all_windows() {
                if let Some(a) = w.window.lookup_action("lock-bar").and_downcast::<gio::SimpleAction>() {
                    a.set_state(&v.to_variant());
                }
            }
        });
        self.add_toggle("show-statusbar", s.show_statusbar, |w, v| {
            config::update(|s| s.show_statusbar = v);
            w.statusbar.set_visible(v);
        });
        self.add_toggle("show-toolbar", s.show_toolbar, |w, v| {
            config::update(|s| s.show_toolbar = v);
            w.toolbar.set_visible(v);
        });
        self.add_toggle("show-locbar", s.show_locbar, |w, v| {
            config::update(|s| s.show_locbar = v);
            w.update_locbar_visibility();
        });
        self.add("visible-columns", |w| w.show_column_chooser());
        self.add("reset-view", |w| {
            if let Some(p) = w.current_pane() {
                p.reset_view();
                p.reload_if_details_missing();
                w.update_for_pane(&p);
            }
        });
        self.add_toggle("sort-desc", s.sort_desc, |w, v| {
            if let Some(p) = w.current_pane() {
                let (by, _) = p.sort_state();
                w.user_sort(&p, &by, v);
            }
        });
        self.add_toggle("split-view", false, |w, v| w.set_split(v));
        self.add("swap-panes", |w| w.swap_panes());
        self.add_toggle("folders-first", s.folders_first, |_, v| {
            config::update(|s| s.folders_first = v);
            for_all_panes(|p| p.set_folders_first(v));
        });
        self.add_radio("view-mode", &s.default_view, |w, v| {
            // Only this tab (and, with per-folder views, this folder); the
            // default for new tabs is set in Preferences.
            if let Some(p) = w.current_pane() {
                if config::with(|s| s.per_folder) {
                    folders::set_view(&p.current().uri(), &v);
                }
                p.set_mode(ViewMode::from_str(&v));
                p.grab_focus();
            }
        });
        self.add_radio("sort-by", &s.sort_by, |w, v| {
            if let Some(p) = w.current_pane() {
                let (_, desc) = p.sort_state();
                w.user_sort(&p, &v, desc);
            }
        });

        // accelerators (user-changeable, see keys.rs)
        if let Some(app) = self.window.application() {
            keys::apply(&app);
        }
    }

    fn setup_signals(self: &Rc<Self>) {
        // A hidden location bar shows while its field or the search is open.
        {
            let w = Rc::downgrade(self);
            self.loc_toggle.connect_toggled(move |_| {
                if let Some(s) = w.upgrade() {
                    s.update_locbar_visibility();
                }
            });
            let w = Rc::downgrade(self);
            self.loc_stack.connect_visible_child_name_notify(move |_| {
                if let Some(s) = w.upgrade() {
                    s.update_locbar_visibility();
                }
            });
        }
        let w = Rc::downgrade(self);
        // Let text entries handle their own keys (Ctrl+C, Delete, ...) before accelerators.
        self.window.connect_key_press_event(|win, ev| {
            if util::is_editable_focused(win.upcast_ref()) {
                let key = ev.keyval();
                // Escape goes to the shortcuts (closing the filter or the
                // location bar), except in the icon views' rename editor,
                // where it cancels renaming (as a shortcut it would move the
                // focus away, which renames).
                let is_escape = key == gdk::keys::constants::Escape;
                // Ctrl+Z (and Redo's Ctrl+Shift+Z, Ctrl+Y) in a text field
                // never undoes or redoes a file operation.
                let is_undo = ev.state().contains(gdk::ModifierType::CONTROL_MASK) && matches!(key.to_lower(), gdk::keys::constants::z | gdk::keys::constants::y);
                if is_undo {
                    win.propagate_key_event(ev);
                    return glib::Propagation::Stop;
                }
                let in_text_view = win.focused_widget().is_some_and(|w| w.is::<gtk::TextView>());
                if (!is_escape || in_text_view) && win.propagate_key_event(ev) {
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        // While typing to select an item, Backspace and Escape edit or end
        // that instead of doing their shortcuts (parent folder, …): the
        // window handles shortcuts before the view gets the key.
        {
            let w = w.clone();
            self.window.connect_key_press_event(move |_, ev| {
                match w.upgrade().and_then(|s| s.current_pane()) {
                    Some(p) if p.typeahead_active() => p.typeahead_key(ev),
                    _ => glib::Propagation::Proceed,
                }
            });
        }
        // Mouse back/forward buttons anywhere in the window.
        {
            let w = w.clone();
            self.window.connect_button_press_event(move |_, ev| {
                let Some(s) = w.upgrade() else { return glib::Propagation::Proceed };
                match crate::pane::nav_of(ev.button()) {
                    Some(crate::pane::Nav::Back) => s.window.activate_action("back", None),
                    Some(crate::pane::Nav::Forward) => s.window.activate_action("forward", None),
                    None => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            });
        }
        // Back-button history popup on right-click.
        {
            let w = w.clone();
            self.back_btn.connect_button_press_event(move |b, ev| {
                if ev.button() != 3 {
                    return glib::Propagation::Proceed;
                }
                let Some(s) = w.upgrade() else { return glib::Propagation::Proceed };
                let Some(p) = s.current_pane() else { return glib::Propagation::Proceed };
                let menu = gtk::Menu::new();
                for (i, f) in p.history_back().iter().take(15).enumerate() {
                    let item = gtk::MenuItem::with_label(&f.title());
                    let pw = Rc::downgrade(&p);
                    item.connect_activate(move |_| {
                        if let Some(p) = pw.upgrade() {
                            p.go_back_n(i + 1);
                        }
                    });
                    menu.append(&item);
                }
                menu.show_all();
                menu.set_attach_widget(Some(b));
                menu.popup_at_widget(b, gdk::Gravity::SouthWest, gdk::Gravity::NorthWest, Some(ev));
                glib::Propagation::Stop
            });
        }
        for nb in [self.notebook.clone(), self.notebook2.clone()] {
            self.setup_notebook_signals(&nb);
        }
        // Focus moving into a pane (e.g. clicking in it) makes it active.
        {
            let w = w.clone();
            self.window.connect_set_focus(move |_, focus| {
                let (Some(s), Some(focus)) = (w.upgrade(), focus) else { return };
                for nb in [s.notebook.clone(), s.notebook2.clone()] {
                    if nb != s.active() && focus.is_ancestor(&nb) {
                        s.set_active(&nb);
                    }
                }
            });
        }
        // Sidebar.
        {
            let w = w.clone();
            self.sidebar.set_handler(move |r| {
                let Some(s) = w.upgrade() else { return };
                match r {
                    crate::sidebar::Request::Open(f, how) => match how {
                        crate::sidebar::OpenIn::Current => s.go(&f),
                        // Like a browser: a middle-clicked place opens in the
                        // background and the current tab stays.
                        crate::sidebar::OpenIn::Tab => {
                            s.open_tab(&f, false);
                        }
                        crate::sidebar::OpenIn::Window => {
                            if let Some(app) = s.window.application() {
                                AppWindow::new(&app, &[f]);
                            }
                        }
                    },
                    crate::sidebar::Request::Drop { uris, dest, action } => s.handle_drop(uris, dest, action),
                    // Leave folders of a device that was unmounted or removed.
                    crate::sidebar::Request::Unmounted(root) => {
                        let panes = s.panes.borrow().clone();
                        for p in panes {
                            let cur = p.current();
                            if cur.equal(&root) || cur.has_prefix(&root) {
                                p.navigate(&gio::File::for_path(glib::home_dir()));
                            }
                        }
                    }
                    crate::sidebar::Request::Status(text) => s.flash_status(&text),
                }
            });
        }
        // The search bar (Ctrl+F), as in Caja: Enter (or its icon) searches
        // the folder and everything below it; on the results it searches
        // again, the text typed meanwhile only changing their title.
        {
            let w = w.clone();
            self.search_field.connect_changed(move |e| {
                let Some(s) = w.upgrade() else { return };
                if s.search_syncing.get() {
                    return;
                }
                if let Some(p) = s.current_pane().filter(|p| p.is_searching()) {
                    p.set_search_text(&e.text());
                    s.window.set_title(&p.title());
                }
            });
        }
        {
            let w = w.clone();
            self.search_field.connect_activate(move |e| {
                let Some(s) = w.upgrade() else { return };
                let text = e.text();
                let Some(p) = s.current_pane() else { return };
                if text.is_empty() {
                    return;
                }
                if p.is_searching() {
                    p.set_search_text(&text);
                    p.reload();
                } else {
                    p.start_search(&text);
                }
            });
            self.search_field.connect_icon_release(|e, pos, _| {
                if pos == gtk::EntryIconPosition::Secondary {
                    e.emit_activate();
                }
            });
        }
        {
            let w = w.clone();
            self.search_btn.connect_toggled(move |b| {
                let Some(s) = w.upgrade() else { return };
                if s.search_syncing.get() {
                    return;
                }
                if b.is_active() {
                    s.show_search();
                    return;
                }
                // Off: from the results back to the folder searched, as in Caja.
                if let Some(p) = s.current_pane().filter(|p| p.is_searching()) {
                    p.navigate(&p.current());
                }
                s.hide_search();
            });
        }
        // Location bar.
        {
            let w = w.clone();
            self.loc_toggle.connect_toggled(move |t| {
                if let Some(s) = w.upgrade() {
                    if t.is_active() {
                        s.loc_stack.set_visible_child_name("entry");
                        if let Some(p) = s.current_pane() {
                            s.set_location_text(&location_text(&p.current()));
                        }
                        s.location_entry.grab_focus();
                        s.location_entry.select_region(0, -1);
                    } else {
                        // With the path always shown as text, the field stays.
                        if config::with(|s| s.path_as_text) {
                            if let Some(p) = s.current_pane() {
                                s.set_location_text(&location_text(&p.current()));
                            }
                            s.location_entry.select_region(0, 0);
                        } else {
                            s.loc_stack.set_visible_child_name("path");
                        }
                        if let Some(p) = s.current_pane() {
                            p.grab_focus();
                        }
                    }
                }
            });
        }
        {
            let w = w.clone();
            self.location_entry.connect_activate(move |e| {
                let Some(s) = w.upgrade() else { return };
                let text = e.text().to_string();
                let text = text.trim();
                if text.is_empty() {
                    return;
                }
                let f = parse_location(text, s.current_pane().map(|p| p.current()));
                match f.query_info("standard::type", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) {
                    Ok(info) if info.file_type() == gio::FileType::Directory => {
                        s.go(&f);
                        s.loc_toggle.set_active(false);
                        if let Some(p) = s.current_pane() {
                            p.grab_focus();
                        }
                    }
                    Ok(_) => {
                        // A file: open its folder and select it.
                        if let (Some(parent), Some(p)) = (f.parent(), s.current_pane()) {
                            p.navigate(&parent);
                            p.select_uris_later(vec![f.uri().to_string()]);
                            s.loc_toggle.set_active(false);
                            p.grab_focus();
                        }
                    }
                    Err(err) => {
                        if !f.is_native() {
                            s.go(&f); // let gvfs try (e.g. sftp://)
                            s.loc_toggle.set_active(false);
                        } else {
                            util::error_dialog(Some(s.window.upcast_ref()), &fill(&tr("Could not find “{text}”"), &[("text", &text)]), err.message());
                        }
                    }
                }
            });
        }
        {
            let w = w.clone();
            // As in Caja: typing at the end completes the folder name, the
            // added part selected; Tab or → takes it.
            self.location_entry.connect_insert_text(move |_, _, _| {
                let Some(s) = w.upgrade() else { return };
                if s.completing.get() {
                    return;
                }
                let ws = Rc::downgrade(&s);
                glib::idle_add_local_once(move || {
                    if let Some(s) = ws.upgrade() {
                        s.complete_location();
                    }
                });
            });
            self.location_entry.connect_key_press_event(|e, ev| {
                if ev.keyval() == gdk::keys::constants::Tab && ev.state() & gtk::accelerator_get_default_mod_mask() == gdk::ModifierType::empty() {
                    if let Some((_, end)) = e.selection_bounds() {
                        if end == e.text_length() as i32 {
                            e.set_position(-1);
                            return glib::Propagation::Stop;
                        }
                    }
                }
                glib::Propagation::Proceed
            });
        }
        {
            let w = w.clone();
            self.location_entry.connect_focus_out_event(move |_, _| {
                if let Some(s) = w.upgrade() {
                    let ws = Rc::downgrade(&s);
                    glib::idle_add_local_once(move || {
                        if let Some(s) = ws.upgrade() {
                            if !s.location_entry.has_focus() && s.window.is_active() {
                                s.loc_toggle.set_active(false);
                            }
                        }
                    });
                }
                glib::Propagation::Proceed
            });
        }
        // Bookmarks file changes (shared with Caja / GTK).
        let bf = gio::File::for_path(util::bookmarks_file());
        if let Ok(m) = bf.monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
            let w = w.clone();
            m.connect_changed(move |_, _, _, _| {
                if let Some(s) = w.upgrade() {
                    s.refresh_bookmarks();
                }
            });
            *self._bookmark_monitor.borrow_mut() = Some(m);
        }
        // Save geometry, clean up.
        {
            let w = w.clone();
            self.window.connect_delete_event(move |win, _| {
                if let Some(s) = w.upgrade() {
                    let (width, height) = win.size();
                    let max = win.is_maximized();
                    let sw = s.paned.position();
                    config::update(|c| {
                        if !max {
                            c.width = width;
                            c.height = height;
                        }
                        c.maximized = max;
                        c.sidebar_width = sw;
                    });
                }
                glib::Propagation::Proceed
            });
        }
        self.window.connect_destroy(move |win| {
            WINDOWS.with(|ws| {
                ws.borrow_mut().retain(|s| {
                    if &s.window == win {
                        for p in s.panes.borrow().iter() {
                            p.shutdown();
                        }
                        false
                    } else {
                        true
                    }
                })
            });
            // The last window closed while operations still run: show them
            // in their own window. They keep Arkha running, and it exits
            // when they're done (the window closes with the last one).
            if WINDOWS.with(|ws| ws.borrow().is_empty()) && ops::is_busy() {
                ops::open_window();
            }
        });
        {
            let w = w.clone();
            self.window.connect_is_active_notify(move |_| {
                if let Some(s) = w.upgrade() {
                    s.update_action_states();
                }
            });
        }
    }

    // ================================================================ tabs

    /// A new tab (Ctrl+T and the like); see open_tab for one opened from
    /// the current tab.
    pub fn add_tab(self: &Rc<Self>, file: &gio::File, switch: bool) -> Rc<Pane> {
        self.insert_tab(file, switch, false)
    }

    /// A tab opened from the current one (a folder or path crumb
    /// middle-clicked, Open in New Tab): placed as Firefox places a link
    /// opened in a new tab, if new tabs go "in-order".
    pub fn open_tab(self: &Rc<Self>, file: &gio::File, switch: bool) -> Rc<Pane> {
        self.insert_tab(file, switch, true)
    }

    fn insert_tab(self: &Rc<Self>, file: &gio::File, switch: bool, from_current: bool) -> Rc<Pane> {
        let pane = Pane::new(file);
        // Tabs can move between windows, so these handlers look up the window
        // that holds the tab now instead of remembering this one.
        let wpane = Rc::downgrade(&pane);
        pane.set_handler(move |ev| {
            if let Some(p) = wpane.upgrade() {
                if let Some(s) = owner_of(&p) {
                    s.on_pane_event(&p, ev);
                }
            }
        });
        // In split view, the pane under the pointer can take focus, once the
        // pointer has stayed there a moment: passing over it on the way to
        // the side pane, say, doesn't count.
        {
            let wp = Rc::downgrade(&pane);
            let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
            pane.connect_pointer_hover(move |inside| {
                let taken = pending.borrow_mut().take();
                if let Some(id) = taken {
                    id.remove();
                }
                if !inside || !config::with(|s| s.focus_follows_mouse) {
                    return;
                }
                let wp = wp.clone();
                let focus = move || {
                    let Some(p) = wp.upgrade() else { return };
                    let (Some(s), Some(nb)) = (owner_of(&p), notebook_of(p.root.upcast_ref())) else { return };
                    // Not while typing a location or a new name.
                    if s.split_on() && nb != s.active() && !util::is_editable_focused(s.window.upcast_ref()) {
                        p.grab_focus();
                    }
                };
                let delay = config::with(|s| s.focus_follows_mouse_delay).max(0) as u64;
                if delay == 0 {
                    focus();
                } else {
                    let done = pending.clone();
                    *pending.borrow_mut() = Some(glib::timeout_add_local_once(std::time::Duration::from_millis(delay), move || {
                        done.borrow_mut().take();
                        focus();
                    }));
                }
            });
        }
        let tab = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        tab.pack_start(&pane.tab_label, true, true, 0);
        let close = gtk::Button::from_icon_name(Some("window-close-symbolic"), gtk::IconSize::Menu);
        close.set_relief(gtk::ReliefStyle::None);
        close.set_focus_on_click(false);
        close.set_tooltip_text(Some(&tr("Close tab")));
        {
            let wp = Rc::downgrade(&pane);
            close.connect_clicked(move |_| {
                if let Some(p) = wp.upgrade() {
                    if let Some(s) = owner_of(&p) {
                        s.close_tab(&p);
                    }
                }
            });
        }
        tab.pack_start(&close, false, false, 0);
        let ev_box = gtk::EventBox::new();
        ev_box.add(&tab);
        {
            let wp = Rc::downgrade(&pane);
            ev_box.connect_button_press_event(move |_, ev| {
                if ev.button() == 2 {
                    if let Some(p) = wp.upgrade() {
                        if let Some(s) = owner_of(&p) {
                            s.close_tab(&p);
                        }
                    }
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
        }
        ev_box.show_all();
        self.panes.borrow_mut().push(pane.clone());
        let nb = self.active();
        let current = self.current_pane().and_then(|c| nb.page_num(&c.root));
        // Only if it's still in this tab bar (tabs can be dragged away).
        let last_opened = self.last_opened.borrow().as_ref().and_then(|w| w.upgrade()).and_then(|w| nb.page_num(&w));
        let at = config::with(|s| tab_index(&s.new_tabs, from_current, current, last_opened));
        let idx = match at {
            Some(i) => nb.insert_page(&pane.root, Some(&ev_box), Some(i)),
            None => nb.append_page(&pane.root, Some(&ev_box)),
        };
        if from_current {
            // Choosing a tab (this one too, if switched to below) starts over.
            *self.last_opened.borrow_mut() = Some(pane.root.upcast_ref::<gtk::Widget>().downgrade());
        }
        self.set_tab_options(pane.root.upcast_ref());
        pane.navigate_initial();
        if switch {
            nb.set_current_page(Some(idx));
            pane.grab_focus();
        }
        self.update_tabs_visibility();
        pane
    }

    fn active(&self) -> gtk::Notebook {
        self.active_nb.borrow().clone()
    }

    fn split_on(&self) -> bool {
        self.notebook2.is_visible()
    }

    /// Makes `nb` the pane the toolbar, menus and shortcuts act on.
    fn set_active(self: &Rc<Self>, nb: &gtk::Notebook) {
        if *self.active_nb.borrow() != *nb {
            self.last_opened.borrow_mut().take();
        }
        *self.active_nb.borrow_mut() = nb.clone();
        self.update_pane_dimming();
        if let Some(p) = self.current_pane() {
            self.update_for_pane(&p);
        }
    }

    fn update_pane_dimming(&self) {
        let (split, active) = (self.split_on(), self.active());
        for nb in [&self.notebook, &self.notebook2] {
            let ctx = nb.style_context();
            if split && *nb != active {
                ctx.add_class("arkha-inactive");
            } else {
                ctx.remove_class("arkha-inactive");
            }
        }
    }

    /// Signals of one pane's notebook: tabs dragged between panes and windows
    /// or out into a new window, switching tabs, and focus activating it.
    fn setup_notebook_signals(self: &Rc<Self>, nb: &gtk::Notebook) {
        let w = Rc::downgrade(self);
        {
            let w = w.clone();
            nb.connect_page_added(move |_, page, _| {
                if let Some(s) = w.upgrade() {
                    s.adopt_page(page);
                }
            });
        }
        {
            let w = w.clone();
            nb.connect_page_removed(move |_, _, _| {
                if let Some(s) = w.upgrade() {
                    s.after_page_removed();
                }
            });
        }
        // A pane with one tab hides its tab bar, which then can't take a
        // drop; show every tab bar while a tab is being dragged.
        nb.connect_drag_begin(|_, _| {
            for w in all_windows() {
                w.notebook.set_show_tabs(true);
                w.notebook2.set_show_tabs(true);
            }
        });
        nb.connect_drag_end(|_, _| {
            glib::idle_add_local_once(|| {
                for w in all_windows() {
                    w.update_tabs_visibility();
                }
            });
        });
        {
            let w = w.clone();
            nb.connect_create_window(move |_, _, x, y| {
                // The notebook only signals while its window exists.
                let app = w.upgrade().and_then(|s| s.window.application()).expect("window of a running app");
                let nw = AppWindow::create(&app, &[], false);
                nw.window.move_(x, y);
                nw.notebook.clone()
            });
        }
        // GTK 3 on Wayland reports a tab dropped where nothing takes it as
        // an error, not as dropped on nothing (gdkselection-wayland.c), so
        // GtkNotebook never opens a window for it: that's done here then,
        // as it would. (The window can't be put where the tab was dropped.)
        {
            let w = w.clone();
            nb.connect_drag_failed(move |nb, _, result| {
                if result != gtk::DragResult::Error || nb.display().type_().name() == "GdkX11Display" {
                    return glib::Propagation::Proceed;
                }
                let (Some(s), Some(page)) = (w.upgrade(), nb.nth_page(nb.current_page())) else { return glib::Propagation::Proceed };
                let Some(app) = s.window.application() else { return glib::Propagation::Proceed };
                let label = nb.tab_label(&page);
                let nw = AppWindow::create(&app, &[], false);
                nb.detach_tab(&page);
                // adopt_page then takes the pane over, as for a dragged tab.
                nw.notebook.append_page(&page, label.as_ref());
                glib::Propagation::Stop
            });
        }
        {
            let w = w.clone();
            nb.connect_switch_page(move |nb, page, _| {
                let Some(s) = w.upgrade() else { return };
                // Tabs opened from now on go after the chosen one.
                s.last_opened.borrow_mut().take();
                if *nb != s.active() {
                    // Switching tabs in the other pane makes it the active one.
                    if s.split_on() {
                        let nb = nb.clone();
                        let s = s.clone();
                        glib::idle_add_local_once(move || s.set_active(&nb));
                    }
                    return;
                }
                let pane = s.panes.borrow().iter().find(|p| p.root.upcast_ref::<gtk::Widget>() == page).cloned();
                if let Some(p) = pane {
                    s.update_for_pane(&p);
                }
            });
        }
    }

    /// A tab left one of the panes (closed, or dragged elsewhere).
    fn after_page_removed(self: &Rc<Self>) {
        if self.moving_tabs.get() {
            return;
        }
        let (n1, n2) = (self.notebook.n_pages(), self.notebook2.n_pages());
        if n1 == 0 && n2 == 0 {
            // Its last tab went to another window. Close once the move has
            // finished.
            let win = self.window.clone();
            glib::idle_add_local_once(move || win.close());
            return;
        }
        if self.split_on() && (n1 == 0 || n2 == 0) {
            let s = self.clone();
            glib::idle_add_local_once(move || s.collapse_split());
        }
        self.update_tabs_visibility();
    }

    /// Turns split view on (the extra pane opens at the current folder) or
    /// off (closing the extra pane's tabs).
    fn set_split(self: &Rc<Self>, on: bool) {
        if on != self.split_on() {
            if on {
                let here = self.current_pane().map(|p| p.current()).unwrap_or_else(util::home_file);
                let (opens, folder, last) = config::with(|s| (s.split_opens.clone(), s.split_folder.clone(), s.split_last.clone()));
                let loc = split_opens_at(&opens, &here, &folder, &last);
                self.notebook2.show();
                let width = self.split.allocated_width();
                if width > 0 {
                    self.split.set_position(width / 2);
                }
                self.set_active(&self.notebook2.clone());
                self.add_tab(&loc, true);
            } else {
                // Keep the pane in use (or, as set, the other one) and close
                // the other one's tabs, noting the folder it showed.
                let keep_active = config::with(|s| s.split_keep != "other");
                let keep_extra = (self.active() == self.notebook2) == keep_active;
                let closing = if keep_extra { self.notebook.clone() } else { self.notebook2.clone() };
                let shown = closing.nth_page(closing.current_page()).and_then(|page| {
                    self.panes.borrow().iter().find(|p| p.root.upcast_ref::<gtk::Widget>() == &page).map(|p| p.current().uri().to_string())
                });
                if let Some(uri) = shown {
                    config::update(|s| s.split_last = uri);
                }
                self.moving_tabs.set(true);
                while let Some(page) = closing.nth_page(Some(0)) {
                    let pane = self.panes.borrow().iter().find(|p| p.root.upcast_ref::<gtk::Widget>() == &page).cloned();
                    // Shut down while still in the window (see Pane::shutdown).
                    if let Some(p) = &pane {
                        p.shutdown();
                    }
                    closing.remove_page(Some(0));
                    if let Some(p) = pane {
                        self.panes.borrow_mut().retain(|q| !Rc::ptr_eq(q, &p));
                    }
                }
                if keep_extra {
                    self.move_tabs_to_main();
                }
                self.notebook2.hide();
                self.moving_tabs.set(false);
                self.set_active(&self.notebook.clone());
                if let Some(p) = self.current_pane() {
                    p.grab_focus();
                }
            }
        }
        self.set_state("split-view", on.to_variant());
        self.set_enabled("swap-panes", on);
        self.swap_btn.set_visible(on && config::with(|s| s.show_swap_button));
        self.update_pane_dimming();
        self.update_tabs_visibility();
    }

    /// Swaps the two sides of split view, each with all its tabs (their
    /// history, selection and scrolling as they are); the pane in use keeps
    /// the focus, and each side keeps its width.
    fn swap_panes(self: &Rc<Self>) {
        if !self.split_on() {
            return;
        }
        let (Some(first), Some(second)) = (self.split.child1(), self.split.child2()) else { return };
        let width = self.split.allocated_width();
        let handle = width - first.allocated_width() - second.allocated_width();
        let position = self.split.position();
        self.split.remove(&first);
        self.split.remove(&second);
        self.split.pack1(&second, true, false);
        self.split.pack2(&first, true, false);
        if width > 0 {
            self.split.set_position((width - position - handle.max(0)).max(0));
        }
        if let Some(p) = self.current_pane() {
            p.grab_focus();
        }
        self.update_pane_dimming();
    }

    /// One side of split view lost its last tab: carry on with a single pane
    /// holding the tabs that are left.
    fn collapse_split(self: &Rc<Self>) {
        if !self.split_on() || (self.notebook.n_pages() > 0 && self.notebook2.n_pages() > 0) {
            return;
        }
        self.notebook2.hide();
        // The main pane is the one that stays, so give it the other's tabs.
        self.moving_tabs.set(true);
        self.move_tabs_to_main();
        self.moving_tabs.set(false);
        self.set_active(&self.notebook.clone());
        self.set_state("split-view", false.to_variant());
        self.update_pane_dimming();
        self.update_tabs_visibility();
    }

    /// Moves the extra pane's tabs (keeping their order and the current one)
    /// to the main pane, which is the one a single-pane window shows.
    /// Callers set `moving_tabs` around it.
    fn move_tabs_to_main(&self) {
        let current = self.notebook2.current_page();
        let offset = self.notebook.n_pages();
        while let Some(page) = self.notebook2.nth_page(Some(0)) {
            let label = self.notebook2.tab_label(&page);
            self.notebook2.remove_page(Some(0));
            self.notebook.append_page(&page, label.as_ref());
            self.set_tab_options(&page);
        }
        if let Some(c) = current {
            self.notebook.set_current_page(Some(offset + c));
        }
    }

    /// The current tab of the pane that isn't active (split view only).
    fn other_pane(&self) -> Option<Rc<Pane>> {
        if !self.split_on() {
            return None;
        }
        let nb = if self.active() == self.notebook { &self.notebook2 } else { &self.notebook };
        let page = nb.nth_page(nb.current_page())?;
        self.panes.borrow().iter().find(|p| p.root.upcast_ref::<gtk::Widget>() == &page).cloned()
    }

    fn transfer_to_other_pane(self: &Rc<Self>, kind: Kind) {
        let (Some(p), Some(other)) = (self.current_pane(), self.other_pane()) else { return };
        let uris = p.selected_uris();
        if !uris.is_empty() {
            self.run_transfer(kind, uris, other.current());
        }
    }

    /// Shows the file operations button while there are operations, with
    /// their overall progress.
    fn update_ops_button(&self) {
        let (n, fraction) = ops::summary();
        // Not with operations shown in their own window (a setting).
        let own_window = config::with(|s| s.ops_window);
        self.ops_btn.set_visible(n > 0 && !own_window);
        self.ops_bar.set_fraction(fraction);
        self.ops_btn.set_tooltip_text(Some(&match n {
            0 => tr("File operations"),
            n => fill(&trn("{n} file operation", "{n} file operations", n as u64), &[("n", &n)]),
        }));
        if n == 0 || own_window {
            self.ops_pop.popdown();
        }
    }

    /// Puts the location bar's button groups in the order from the settings.
    fn apply_bar_order(&self) {
        let items = [
            ("path", self.path_area.clone().upcast::<gtk::Widget>()),
            ("zoom", self.zoom_box.clone().upcast()),
            ("view", self.view_box.clone().upcast()),
            ("split", self.split_box.clone().upcast()),
            ("info", self.info_btn.clone().upcast()),
            ("ops", self.ops_btn.clone().upcast()),
        ];
        let Some(bar) = self.path_area.parent().and_then(|p| p.downcast::<gtk::Box>().ok()) else { return };
        for (i, id) in config::get().bar_order.iter().enumerate() {
            if let Some((_, w)) = items.iter().find(|(wid, _)| wid == id) {
                bar.reorder_child(w, i as i32);
            }
        }
    }

    fn set_tab_options(&self, page: &gtk::Widget) {
        let Some(nb) = notebook_of(page) else { return };
        nb.set_tab_reorderable(page, true);
        nb.set_tab_detachable(page, true);
        // Tabs share the bar equally; past their minimum width the bar scrolls.
        nb.set_child_tab_expand(page, true);
    }

    /// A tab was dropped into this window from another one: move its pane
    /// over to this window's list.
    fn adopt_page(self: &Rc<Self>, page: &gtk::Widget) {
        if self.panes.borrow().iter().any(|p| p.root.upcast_ref::<gtk::Widget>() == page) {
            return; // one of ours (added by add_tab)
        }
        for other in all_windows() {
            if Rc::ptr_eq(&other, self) {
                continue;
            }
            let pos = other.panes.borrow().iter().position(|p| p.root.upcast_ref::<gtk::Widget>() == page);
            if let Some(pos) = pos {
                let pane = other.panes.borrow_mut().remove(pos);
                self.panes.borrow_mut().push(pane.clone());
                self.set_tab_options(page);
                if let Some(nb) = notebook_of(page) {
                    nb.set_current_page(nb.page_num(page));
                    self.set_active(&nb);
                }
                self.update_tabs_visibility();
                other.update_tabs_visibility();
                self.update_for_pane(&pane);
                self.window.present();
                pane.grab_focus();
                return;
            }
        }
    }

    fn close_tab(self: &Rc<Self>, pane: &Rc<Pane>) {
        if self.panes.borrow().len() <= 1 {
            self.window.close();
            return;
        }
        let nb = notebook_of(pane.root.upcast_ref()).unwrap_or_else(|| self.notebook.clone());
        if let Some(n) = nb.page_num(&pane.root) {
            let (back, fwd) = pane.history();
            let mut closed = self.closed_tabs.borrow_mut();
            closed.push(ClosedTab { place: pane.place(), back, fwd, position: n });
            if closed.len() > MAX_CLOSED_TABS {
                closed.remove(0);
            }
            drop(closed);
            // The focus leaves the tab while it's still there: moved only
            // after its view is gone, accessibility (which follows the
            // focus) gets the gone view handed to it (gtk_widget_get_parent
            // assertion).
            if self.window.focused_widget().is_some_and(|f| f.is_ancestor(&pane.root)) {
                self.window.set_focus(None::<&gtk::Widget>);
            }
            // Shut down while still in the window (see Pane::shutdown).
            pane.shutdown();
            nb.remove_page(Some(n));
        }
        self.set_enabled("reopen-tab", true);
        // (Done already if it was in a notebook.)
        pane.shutdown();
        self.panes.borrow_mut().retain(|p| !Rc::ptr_eq(p, pane));
        self.update_tabs_visibility();
        if let Some(p) = self.current_pane() {
            self.update_for_pane(&p);
            p.grab_focus();
        }
    }

    /// Brings back the most recently closed tab where it was, with its history.
    fn reopen_tab(self: &Rc<Self>) {
        let Some(t) = self.closed_tabs.borrow_mut().pop() else { return };
        let pane = self.add_tab(&t.place.folder, true);
        pane.set_history(t.back, t.fwd);
        if t.place.search.is_some() {
            pane.show_place(t.place);
        }
        let nb = self.active();
        let last = nb.n_pages().saturating_sub(1);
        nb.reorder_child(&pane.root, Some(t.position.min(last)));
        self.set_enabled("reopen-tab", !self.closed_tabs.borrow().is_empty());
        self.update_for_pane(&pane);
    }

    fn update_tabs_visibility(&self) {
        let always = config::with(|s| s.always_show_tabs);
        for nb in [&self.notebook, &self.notebook2] {
            nb.set_show_tabs(nb.n_pages() > 1 || always);
        }
    }

    pub fn current_pane(&self) -> Option<Rc<Pane>> {
        let nb = self.active();
        let n = nb.current_page()?;
        let page = nb.nth_page(Some(n))?;
        self.panes.borrow().iter().find(|p| p.root.upcast_ref::<gtk::Widget>() == &page).cloned()
    }

    fn is_current(&self, pane: &Rc<Pane>) -> bool {
        self.current_pane().map(|c| Rc::ptr_eq(&c, pane)).unwrap_or(false)
    }

    pub fn go(self: &Rc<Self>, f: &gio::File) {
        if let Some(p) = self.current_pane() {
            p.navigate(f);
            p.grab_focus();
        } else {
            self.add_tab(f, true);
        }
    }

    // ================================================================ pane events

    fn on_pane_event(self: &Rc<Self>, pane: &Rc<Pane>, ev: Event) {
        match ev {
            Event::Location => {
                if self.is_current(pane) {
                    self.update_for_pane(pane);
                }
            }
            Event::Selection => {
                if self.is_current(pane) {
                    self.status.set_text(&pane.status_text());
                    self.update_action_states();
                }
            }
            Event::Loading(on) => {
                if self.is_current(pane) {
                    self.set_enabled("stop", on);
                    if on {
                        self.spinner.start();
                    } else {
                        self.spinner.stop();
                    }
                }
            }
            Event::Activate => self.open_selection(OpenMode::Normal),
            Event::Context { event, on_item } => self.context_menu(pane, event, on_item),
            Event::OpenNewTab(f) => {
                self.open_tab(&f, false);
            }
            Event::OpenFile(r) => {
                // As when opening it normally: files in the trash show their
                // properties instead.
                if pane.is_trash() {
                    props::show(self.window.upcast_ref(), vec![gio::File::for_uri(&r.uri)]);
                } else {
                    self.launch_files(&[r]);
                }
            }
            Event::Back => pane.go_back(),
            Event::Forward => pane.go_forward(),
            Event::Drop { uris, dest, action } => self.handle_drop(uris, dest, action),
            Event::Rename { uri, new_name } => self.rename(pane, &uri, &new_name),
            Event::Sort { by, desc } => self.user_sort(pane, &by, desc),
            Event::ColumnsChanged => show_chosen_columns(),
        }
    }

    /// A sort order the user picked in `pane`: remembered for its folder (and
    /// applied to other tabs showing it) with per-folder views on, otherwise
    /// the new sorting everywhere.
    /// View → Visible Columns, as in Caja.
    fn show_column_chooser(&self) {
        let d = gtk::Dialog::with_buttons(Some(&tr("Visible Columns")), Some(&self.window), gtk::DialogFlags::DESTROY_WITH_PARENT, &[(tr("_Close").as_str(), gtk::ResponseType::Close)]);
        let area = d.content_area();
        area.set_spacing(12);
        area.set_border_width(12);
        let text = gtk::Label::new(Some(&tr("Choose the order of information to appear in the list view.")));
        text.set_xalign(0.0);
        area.add(&text);
        area.add(&column_chooser());
        d.set_default_size(420, -1);
        d.connect_response(|d, _| d.close());
        d.show_all();
    }

    fn user_sort(self: &Rc<Self>, pane: &Rc<Pane>, by: &str, desc: bool) {
        if config::with(|s| s.per_folder) {
            let folder = pane.current();
            folders::set_sort(&folder.uri(), by, desc);
            for_all_panes(|p| {
                if p.current().equal(&folder) {
                    p.apply_sort(by, desc);
                    p.reload_if_details_missing();
                }
            });
        } else {
            config::update(|s| {
                s.sort_by = by.to_string();
                s.sort_desc = desc;
            });
            for_all_panes(|p| {
                p.apply_sort(by, desc);
                p.reload_if_details_missing();
            });
        }
        sync_all_windows();
    }

    fn update_for_pane(self: &Rc<Self>, pane: &Rc<Pane>) {
        self.sync_search(pane);
        let f = pane.current();
        self.window.set_title(&pane.title());
        self.update_crumbs(&f);
        if self.loc_stack.visible_child_name().as_deref() == Some("entry") && !self.location_entry.has_focus() {
            self.set_location_text(&location_text(&f));
        }
        self.sidebar.set_location(&f);
        self.set_state("view-mode", pane.mode().as_str().to_variant());
        self.set_state("info-panel", pane.info_panel_on().to_variant());
        let (by, desc) = pane.sort_state();
        self.set_state("sort-by", by.to_variant());
        self.set_state("sort-desc", desc.to_variant());
        self.status.set_text(&pane.status_text());
        if pane.is_loading() {
            self.spinner.start();
        } else {
            self.spinner.stop();
        }
        self.update_action_states_for(pane);
    }

    fn update_action_states(&self) {
        if let Some(p) = self.current_pane() {
            self.update_action_states_for(&p);
        }
    }

    /// The actions as `p` allows them. `p` is passed in when switching
    /// tabs: GTK still reports the tab left as the current one then, and
    /// a search's results there left Paste off in the tab switched to.
    fn update_action_states_for(&self, p: &Rc<Pane>) {
        let sel = p.selected();
        let has = !sel.is_empty();
        let in_trash = p.is_trash();
        let searching = p.is_searching();
        let writable = !in_trash;
        self.set_enabled("back", p.can_back());
        self.set_enabled("forward", p.can_forward());
        self.set_enabled("up", p.current().parent().is_some());
        for a in ["cut", "copy", "trash", "delete", "copy-to", "move-to"] {
            self.set_enabled(a, has);
        }
        let other_ok = self.other_pane().is_some_and(|o| !o.is_trash() && !o.current().equal(&p.current()));
        self.set_enabled("copy-to-pane", has && other_ok);
        self.set_enabled("move-to-pane", has && writable && other_ok);
        for a in ["open", "open-with-other"] {
            self.set_enabled(a, has);
        }
        let dirs = has && sel.iter().all(|r| r.is_dir);
        self.set_enabled("open-new-tab", dirs);
        self.set_enabled("open-new-window", dirs);
        // Bulk renaming works on local paths, like caja-rename.
        let local = has && !in_trash && sel.iter().all(|r| gio::File::for_uri(&r.uri).path().is_some());
        // Rename with several items opens bulk renaming; with one it renames
        // in place (the bulk window is then in the context menu only).
        self.set_enabled("rename", (sel.len() == 1 && !in_trash) || (sel.len() > 1 && local));
        self.set_enabled("rename-all", local);
        self.set_enabled("duplicate", has && writable);
        self.set_enabled("make-link", has && writable && sel.iter().all(|r| gio::File::for_uri(&r.uri).is_native()));
        // Search results are no folder to put anything in, as in Caja.
        self.set_enabled("paste", writable && !searching);
        self.set_enabled("paste-check", writable && !searching);
        self.set_enabled("paste-into", sel.len() == 1 && dirs && writable);
        self.set_enabled("new-folder", writable && !searching);
        self.set_enabled("new-file", writable && !searching);
        self.set_enabled("new-from-template", writable && !searching);
        self.set_enabled("restore", has && in_trash && sel.iter().all(|r| !r.orig.is_empty()));
        self.set_enabled("terminal", p.current().path().is_some());
        self.set_enabled("stop", p.is_loading());
        self.set_enabled("switch-pane", self.split_on());
        self.set_enabled("swap-panes", self.split_on());
        self.set_enabled("same-location", self.other_pane().is_some_and(|o| !o.current().equal(&p.current())));
        let nb = self.active();
        let (cur, n) = (nb.page_num(&p.root).unwrap_or(0), nb.n_pages());
        self.set_enabled("tab-left", cur > 0);
        self.set_enabled("tab-right", cur + 1 < n);
    }

    fn flash_status(self: &Rc<Self>, text: &str) {
        self.status.set_text(text);
        let w = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_secs(3), move || {
            if let Some(s) = w.upgrade() {
                if let Some(p) = s.current_pane() {
                    s.status.set_text(&p.status_text());
                }
            }
        });
    }

    // ================================================================ path bar

    fn update_crumbs(self: &Rc<Self>, f: &gio::File) {
        // Keep deeper crumbs if we moved to an ancestor in the same chain.
        let keep = self.crumbs.borrow().iter().any(|c| c.equal(f));
        if !keep {
            let mut chain = vec![];
            let home = util::home_file();
            let mut cur = Some(f.clone());
            while let Some(c) = cur {
                chain.push(c.clone());
                if c.equal(&home) {
                    break;
                }
                cur = c.parent();
            }
            chain.reverse();
            *self.crumbs.borrow_mut() = chain;
        }
        for c in self.path_box.children() {
            self.path_box.remove(&c);
        }
        let home = util::home_file();
        let mut active_btn = None;
        let crumbs = self.crumbs.borrow().clone();
        for c in crumbs.iter() {
            let b = gtk::ToggleButton::new();
            // Caja's classes: the theme pads them as buttons with text.
            b.style_context().add_class("text-button");
            let bx = gtk::Box::new(gtk::Orientation::Horizontal, 2);
            let (icon, label) = if c.equal(&home) {
                (Some("user-home"), tr("Home"))
            } else if c.uri() == "trash:///" {
                (Some("user-trash"), tr("Trash"))
            } else if c.parent().is_none() {
                // A mounted server's files have local paths too (GVfs's
                // FUSE folder): only a native top is the file system.
                if c.is_native() {
                    (Some("drive-harddisk"), tr("File System"))
                } else {
                    // The mount's name, as the side pane shows it ("debian").
                    let name = c.find_enclosing_mount(gio::Cancellable::NONE).map(|m| m.name().to_string());
                    (Some("folder-remote"), name.unwrap_or_else(|_| util::display_name(c)))
                }
            } else {
                (None, util::display_name(c))
            };
            if let Some(i) = icon {
                b.style_context().add_class("image-button");
                bx.pack_start(&gtk::Image::from_icon_name(Some(i), gtk::IconSize::Menu), false, false, 0);
            }
            let l = gtk::Label::new(Some(&label));
            l.set_ellipsize(pango::EllipsizeMode::Middle);
            l.set_max_width_chars(28);
            bx.pack_start(&l, false, false, 0);
            b.add(&bx);
            b.set_focus_on_click(false);
            let is_active = c.equal(f);
            b.set_active(is_active);
            let bold = pango::AttrList::new();
            bold.insert(pango::AttrInt::new_weight(pango::Weight::Bold));
            if is_active {
                active_btn = Some(b.clone());
                l.set_attributes(Some(&bold));
            } else {
                // Room for the name in bold, as in Caja: a button keeps its
                // width when its folder becomes the open one.
                let layout = l.create_pango_layout(Some(&label));
                let normal = layout.pixel_size().0;
                layout.set_attributes(Some(&bold));
                let wide = layout.pixel_size().0;
                l.set_margin_start((wide - normal) / 2);
                l.set_margin_end((wide - normal + 1) / 2);
            }
            let w = Rc::downgrade(self);
            let target = c.clone();
            b.connect_clicked(move |b| {
                if let Some(s) = w.upgrade() {
                    let cur = s.current_pane().map(|p| p.current());
                    if cur.map(|c| c.equal(&target)).unwrap_or(false) {
                        b.set_active(true);
                        return;
                    }
                    s.go(&target);
                }
            });
            // A middle-click opens the folder in a new tab, in the background,
            // as in Caja and as a middle-clicked place in the side pane. The
            // press is kept from the path bar, whose middle-click autoscrolls.
            b.connect_button_press_event(|_, ev| match ev.button() {
                2 => glib::Propagation::Stop,
                _ => glib::Propagation::Proceed,
            });
            {
                let w = Rc::downgrade(self);
                let target = c.clone();
                b.connect_button_release_event(move |b, ev| {
                    if ev.button() != 2 {
                        return glib::Propagation::Proceed;
                    }
                    // Only if let go over the crumb, as with a click.
                    let (x, y) = ev.position();
                    let inside = x >= 0.0 && y >= 0.0 && x < b.allocated_width() as f64 && y < b.allocated_height() as f64;
                    if let (true, Some(s)) = (inside, w.upgrade()) {
                        s.open_tab(&target, false);
                    }
                    glib::Propagation::Stop
                });
            }
            // Drop onto crumbs.
            // No DestDefaults::DROP: it finishes with delete=true on a move,
            // which would make the source view drop its row.
            // No DestDefaults::MOTION either: it refuses other drags instead
            // of letting the widgets around the crumb take them.
            b.drag_dest_set(
                gtk::DestDefaults::empty(),
                &crate::pane::drop_targets(),
                gdk::DragAction::COPY | gdk::DragAction::MOVE | gdk::DragAction::LINK,
            );
            {
                // Same rules as dropping into a file view (Ctrl copies, a
                // plain drag moves on the same filesystem, ...).
                let target = c.clone();
                b.connect_drag_motion(move |b, ctx, _, _, time| {
                    if b.drag_dest_find_target(ctx, None).is_none() {
                        return false;
                    }
                    ctx.drag_status(crate::pane::choose_action(b.upcast_ref(), ctx, &target), time);
                    b.drag_highlight();
                    true
                });
            }
            b.connect_drag_leave(|b, _, _| b.drag_unhighlight());
            let target = c.clone();
            b.connect_drag_drop(move |b, ctx, _, _, time| {
                match b.drag_dest_find_target(ctx, None) {
                    // Out of an archive: the archive manager saves the files.
                    Some(t) if crate::xds::is(&t) => {
                        if !crate::xds::start(b, ctx, &target, time) {
                            ctx.drag_finish(false, false, time);
                        }
                        true
                    }
                    // Out of Ark: Ark extracts them.
                    Some(t) if crate::arkdnd::is(&t) => {
                        crate::arkdnd::start(b, ctx, &target, time);
                        true
                    }
                    Some(t) => {
                        b.drag_get_data(ctx, &t, time);
                        true
                    }
                    // Not files (e.g. a location bar button group being
                    // rearranged): let the path area around it take it.
                    None => false,
                }
            });
            {
                let w = Rc::downgrade(self);
                let target = c.clone();
                b.connect_drag_data_received(move |b, ctx, _, _, sel, _, time| {
                    if crate::xds::is(&sel.target()) {
                        crate::xds::finish(ctx, sel, time);
                        return;
                    }
                    if crate::arkdnd::received(b, ctx, sel, time) {
                        return;
                    }
                    let uris: Vec<String> = sel.uris().iter().map(|u| u.to_string()).collect();
                    let action = ctx.selected_action();
                    ctx.drag_finish(!uris.is_empty(), false, time);
                    if let Some(s) = w.upgrade() {
                        if !uris.is_empty() {
                            s.handle_drop(uris, target.clone(), action);
                        }
                    }
                });
            }
            self.path_box.pack_start(&b, false, false, 0);
        }
        self.path_box.show_all();
        if let Some(b) = active_btn {
            let sw = self.path_scroll.clone();
            glib::idle_add_local_once(move || {
                // The window may have been closed meanwhile: a destroyed
                // scrolled window has no scrollbar (and no adjustment).
                // Nor if it's not in a shown window (see
                // Pane::scroll_columns_to_end).
                if b.parent().is_none() || sw.hscrollbar().is_none() || !sw.is_realized() {
                    return;
                }
                let adj = sw.hadjustment();
                let alloc = b.allocation();
                let right = (alloc.x() + alloc.width()) as f64;
                if right > adj.value() + adj.page_size() {
                    adj.set_value(right - adj.page_size());
                } else if (alloc.x() as f64) < adj.value() {
                    adj.set_value(alloc.x() as f64);
                }
            });
        }
    }

    fn search_open(&self) -> bool {
        self.loc_stack.visible_child_name().as_deref() == Some("search")
    }

    /// Updates the search widgets without their handlers acting on it.
    fn syncing_search(&self, f: impl FnOnce()) {
        self.search_syncing.set(true);
        f();
        self.search_syncing.set(false);
    }

    /// Shows the search bar (Ctrl+F, the toolbar's button), empty, for the
    /// current pane's folder; it just takes the focus if it's shown.
    fn show_search(self: &Rc<Self>) {
        let Some(p) = self.current_pane() else { return };
        if !self.search_open() {
            self.loc_toggle.set_active(false);
            *self.search_pane.borrow_mut() = Some((Rc::downgrade(&p), p.current().uri().to_string()));
            self.syncing_search(|| self.search_field.set_text(""));
            self.loc_stack.set_visible_child_name("search");
        }
        self.syncing_search(|| self.search_btn.set_active(true));
        self.search_field.grab_focus();
    }

    /// Back to the path (or the location field).
    fn hide_search(&self) {
        self.search_pane.borrow_mut().take();
        self.loc_stack.set_visible_child_name(if config::with(|s| s.path_as_text) { "entry" } else { "path" });
        self.syncing_search(|| self.search_btn.set_active(false));
    }

    /// The search bar for `pane`, as in Caja: shown with the search's text
    /// while its results are, and one opened with Ctrl+F stays only while
    /// its pane shows the folder it was opened in.
    fn sync_search(&self, pane: &Rc<Pane>) {
        if let Some(text) = pane.search_text() {
            if !self.search_open() || self.search_field.text() != text {
                self.syncing_search(|| self.search_field.set_text(&text));
            }
            self.search_pane.borrow_mut().take();
            self.loc_stack.set_visible_child_name("search");
            self.syncing_search(|| self.search_btn.set_active(true));
            return;
        }
        if !self.search_open() {
            return;
        }
        let keep = self.search_pane.borrow().as_ref().is_some_and(|(w, uri)| w.upgrade().is_some_and(|p| Rc::ptr_eq(&p, pane)) && *uri == pane.current().uri());
        if !keep {
            self.hide_search();
        }
    }

    /// Breadcrumbs with an edit toggle, or (path_as_text) always the text
    /// field, without the toggle.
    /// The location bar shows if Preferences say so, or else while the
    /// location field (Ctrl+L) or the search (Ctrl+F) is open, as in Caja.
    fn update_locbar_visibility(&self) {
        let wanted = config::with(|s| s.show_locbar);
        let in_use = self.loc_toggle.is_active() || self.search_open();
        self.locbar.set_visible(wanted || in_use);
    }

    fn apply_path_mode(&self) {
        let text = config::with(|s| s.path_as_text);
        self.loc_toggle.set_visible(!text);
        if self.search_open() {
            return;
        }
        if text {
            self.loc_stack.set_visible_child_name("entry");
            if let Some(p) = self.current_pane() {
                self.set_location_text(&location_text(&p.current()));
            }
        } else if !self.loc_toggle.is_active() {
            self.loc_stack.set_visible_child_name("path");
        }
    }

    fn show_location_entry(self: &Rc<Self>, text: Option<&str>) {
        self.loc_toggle.set_active(true);
        if let Some(t) = text {
            self.set_location_text(t);
            self.location_entry.set_position(-1);
        } else {
            self.location_entry.grab_focus();
            self.location_entry.select_region(0, -1);
        }
    }

    /// Set the entry text without triggering completion.
    fn set_location_text(&self, t: &str) {
        self.completing.set(true);
        self.location_entry.set_text(t);
        self.completing.set(false);
        // Folders may have changed since the last time.
        self.completion_dir.replace(None);
    }

    /// Caja's location completion: when the cursor is at the end of what's
    /// typed, add the rest of the folder name it's the start of, selected.
    fn complete_location(&self) {
        let e = &self.location_entry;
        let len = e.text_length() as i32;
        if !e.has_focus() || e.position() != len || e.selection_bounds().is_some() {
            return;
        }
        let text = e.text().to_string();
        let (dir_text, base) = match text.rfind('/') {
            Some(i) => (&text[..=i], &text[i + 1..]),
            None => ("", text.as_str()),
        };
        if base.is_empty() || text.contains("://") {
            return;
        }
        // The folder the name is typed in: absolute, in ~, or relative to
        // the open folder.
        let dir = if dir_text.starts_with('/') {
            std::path::PathBuf::from(dir_text)
        } else if let Some(rest) = dir_text.strip_prefix("~/") {
            glib::home_dir().join(rest)
        } else if dir_text.starts_with('~') {
            // ~user/… isn't supported.
            return;
        } else {
            match self.current_pane().and_then(|p| p.current().path()) {
                Some(cur) => cur.join(dir_text),
                None => return,
            }
        };
        let stale = self.completion_dir.borrow().as_ref().is_none_or(|(d, _)| *d != dir);
        if stale {
            let names = std::fs::read_dir(&dir)
                .map(|rd| {
                    // The listing knows each entry's type: only links need
                    // a look at what they point to (a round trip each on a
                    // network share).
                    rd.flatten()
                        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() || (t.is_symlink() && e.path().is_dir())))
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect()
                })
                .unwrap_or_default();
            self.completion_dir.replace(Some((dir, names)));
        }
        let suffix = self.completion_dir.borrow().as_ref().and_then(|(_, names)| completion_suffix(base, names));
        if let Some(suffix) = suffix {
            self.completing.set(true);
            let mut pos = len;
            e.insert_text(&suffix, &mut pos);
            self.completing.set(false);
            e.select_region(len, -1);
        }
    }

    // ================================================================ commands

    fn open_selection(self: &Rc<Self>, mode: OpenMode) {
        let Some(p) = self.current_pane() else { return };
        let sel = p.selected();
        if sel.is_empty() {
            return;
        }
        if p.is_trash() {
            // Opening files from the trash makes little sense; show properties instead.
            if sel.iter().all(|r| !r.is_dir) {
                props::show(self.window.upcast_ref(), p.selected_files());
                return;
            }
        }
        let (dirs, files): (Vec<_>, Vec<_>) = sel.into_iter().partition(|r| r.is_dir);
        let mut first = matches!(mode, OpenMode::Normal);
        for d in &dirs {
            let f = gio::File::for_uri(&d.uri);
            match mode {
                OpenMode::Window => {
                    if let Some(app) = self.window.application() {
                        AppWindow::new(&app, &[f]);
                    }
                }
                _ if first => {
                    first = false;
                    p.navigate(&f);
                }
                _ => {
                    self.open_tab(&f, false);
                }
            }
        }
        if !files.is_empty() {
            self.launch_files(&files);
        }
    }

    fn launch_files(self: &Rc<Self>, files: &[crate::pane::RowInfo]) {
        // Executable scripts and programs: ask, like Caja.
        if files.len() == 1 {
            let f = gio::File::for_uri(&files[0].uri);
            let ct = &files[0].ctype;
            let execable = f
                .query_info("access::can-execute", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
                .map(|i| i.boolean("access::can-execute"))
                .unwrap_or(false);
            let is_prog = ct == "application/x-executable"
                || ct == "application/x-sharedlib"
                || ct == "application/x-pie-executable"
                || ct == "application/vnd.appimage";
            let is_script = gio::content_type_is_a(ct, "text/plain") && execable;
            if execable && f.path().is_some() && (is_prog || is_script) {
                self.ask_run(&f, is_script);
                return;
            }
        }
        // Group by default application.
        let mut groups: Vec<(gio::AppInfo, Vec<gio::File>)> = vec![];
        let mut unknown = vec![];
        for r in files {
            match gio::AppInfo::default_for_type(&r.ctype, false) {
                Some(app) => {
                    let id = app.id().map(|s| s.to_string());
                    match groups.iter_mut().find(|(a, _)| a.id().map(|s| s.to_string()) == id) {
                        Some((_, v)) => v.push(gio::File::for_uri(&r.uri)),
                        None => groups.push((app, vec![gio::File::for_uri(&r.uri)])),
                    }
                }
                None => unknown.push(r.clone()),
            }
        }
        for (app, files) in groups {
            self.launch_app(&app, &files);
        }
        if let Some(r) = unknown.first() {
            self.choose_app(&r.ctype, unknown.iter().map(|r| gio::File::for_uri(&r.uri)).collect());
        }
    }

    fn launch_app(&self, app: &gio::AppInfo, files: &[gio::File]) {
        let ctx = self.window.display().app_launch_context();
        let res = if app.supports_files() || files.iter().all(|f| f.is_native()) {
            app.launch(files, ctx.as_ref())
        } else {
            let uris: Vec<String> = files.iter().map(|f| f.uri().to_string()).collect();
            let refs: Vec<&str> = uris.iter().map(|s| s.as_str()).collect();
            app.launch_uris(&refs, ctx.as_ref())
        };
        if let Err(e) = res {
            util::error_dialog(Some(self.window.upcast_ref()), &fill(&tr("Could not open with {program}"), &[("program", &app.display_name())]), e.message());
        }
    }

    fn ask_run(self: &Rc<Self>, f: &gio::File, is_script: bool) {
        // Direction marks shown, not obeyed: "gpj.exe" can't pass for "exe.jpg".
        let name = util::visible_controls(&util::display_name(f));
        let d = gtk::MessageDialog::new(
            Some(&self.window),
            gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
            gtk::MessageType::Question,
            gtk::ButtonsType::None,
            &fill(&tr("Do you want to run “{name}”, or display its contents?"), &[("name", &name)]),
        );
        d.set_secondary_text(Some(&fill(&tr("“{name}” is an executable file."), &[("name", &name)])));
        let term = util::terminal_command();
        if term.is_some() {
            d.add_button(&tr("Run in _Terminal"), gtk::ResponseType::Other(1));
        }
        if is_script {
            d.add_button(&tr("_Display"), gtk::ResponseType::Other(2));
        }
        d.add_button(&tr("_Cancel"), gtk::ResponseType::Cancel);
        d.add_button(&tr("_Run"), gtk::ResponseType::Other(3));
        // Enter doesn't run it: on FAT-formatted drives every file is
        // executable, a "text file" among them too.
        d.set_default_response(gtk::ResponseType::Cancel);
        let w = Rc::downgrade(self);
        let f = f.clone();
        d.connect_response(move |d, r| {
            d.close();
            let Some(s) = w.upgrade() else { return };
            let path = f.path().unwrap();
            let dir = path.parent().map(|p| p.to_path_buf());
            let res = match r {
                gtk::ResponseType::Other(1) => {
                    let t = term.clone().unwrap();
                    let argv = util::terminal_run_argv(&t, path.as_os_str());
                    util::spawn_in_dir(&argv, dir.as_deref())
                }
                gtk::ResponseType::Other(2) => {
                    if let Some(app) = gio::AppInfo::default_for_type("text/plain", false) {
                        s.launch_app(&app, std::slice::from_ref(&f));
                    }
                    Ok(())
                }
                gtk::ResponseType::Other(3) => util::spawn_in_dir(&[path.as_os_str()], dir.as_deref()),
                _ => Ok(()),
            };
            if let Err(e) = res {
                util::error_dialog(Some(s.window.upcast_ref()), &tr("Could not run the program"), &e);
            }
        });
        d.show();
    }

    fn choose_app(self: &Rc<Self>, ctype: &str, files: Vec<gio::File>) {
        let d = gtk::AppChooserDialog::for_content_type(Some(&self.window), gtk::DialogFlags::MODAL, ctype);
        if let Ok(w) = d.widget().downcast::<gtk::AppChooserWidget>() {
            w.set_show_fallback(true);
            w.set_show_other(true);
        }
        let ws = Rc::downgrade(self);
        d.connect_response(move |d, r| {
            if r == gtk::ResponseType::Ok {
                if let (Some(app), Some(s)) = (d.app_info(), ws.upgrade()) {
                    s.launch_app(&app, &files);
                }
            }
            d.close();
        });
        d.show();
    }

    fn open_with(self: &Rc<Self>, id: &str) {
        let Some(p) = self.current_pane() else { return };
        let Some(app) = gio::DesktopAppInfo::new(id) else { return };
        self.launch_app(app.upcast_ref(), &p.selected_files());
    }

    fn open_with_other(self: &Rc<Self>) {
        let Some(p) = self.current_pane() else { return };
        let sel = p.selected();
        if let Some(r) = sel.first() {
            self.choose_app(&r.ctype.clone(), p.selected_files());
        }
    }

    fn new_folder(self: &Rc<Self>) {
        let Some(p) = self.current_pane() else { return };
        let dir = p.current();
        let name = util::unique_name(&dir, &tr("Untitled Folder"), true);
        let f = dir.child(&name);
        match f.make_directory(gio::Cancellable::NONE) {
            Ok(()) => {
                ops::note_created(&f, true, None);
                p.add_file_now(&f);
                p.rename_later(f.uri().to_string());
            }
            Err(e) => util::error_dialog(Some(self.window.upcast_ref()), &tr("Could not create the folder"), e.message()),
        }
    }

    fn new_file(self: &Rc<Self>, template: Option<String>) {
        let Some(p) = self.current_pane() else { return };
        let dir = p.current();
        let res = match &template {
            None => {
                let name = util::unique_name(&dir, &tr("Untitled Document"), false);
                let f = dir.child(&name);
                f.create(gio::FileCreateFlags::NONE, gio::Cancellable::NONE).map(|_| f)
            }
            Some(t) => {
                let src = gio::File::for_path(t);
                let base = src.basename().map(|b| b.to_string_lossy().to_string()).unwrap_or_default();
                let name = util::unique_name(&dir, &base, false);
                let f = dir.child(&name);
                src.copy(&f, gio::FileCopyFlags::NONE, gio::Cancellable::NONE, None).map(|_| f)
            }
        };
        match res {
            Ok(f) => {
                ops::note_created(&f, false, template);
                p.add_file_now(&f);
                p.rename_later(f.uri().to_string());
            }
            Err(e) => util::error_dialog(Some(self.window.upcast_ref()), &tr("Could not create the document"), e.message()),
        }
    }

    fn rename(self: &Rc<Self>, pane: &Rc<Pane>, uri: &str, new_name: &str) {
        if new_name.contains('/') {
            util::error_dialog(Some(self.window.upcast_ref()), &tr("Invalid name"), &tr("File names cannot contain “/”."));
            return;
        }
        let f = gio::File::for_uri(uri);
        if let Some(parent) = f.parent() {
            let target = parent.child(new_name);
            if target.query_exists(gio::Cancellable::NONE) && !target.equal(&f) {
                util::error_dialog(
                    Some(self.window.upcast_ref()),
                    &fill(&tr("The name “{name}” is already used in this location."), &[("name", &new_name)]),
                    &tr("Please use a different name."),
                );
                return;
            }
        }
        match f.set_display_name(new_name, gio::Cancellable::NONE) {
            Ok(nf) => {
                ops::note_rename(&f, &nf);
                pane.refresh_file(&f);
                pane.add_file_now(&nf);
                pane.select_uris(&[nf.uri().to_string()]);
            }
            Err(e) => util::error_dialog(Some(self.window.upcast_ref()), &fill(&tr("Could not rename “{name}”"), &[("name", &util::display_name(&f))]), e.message()),
        }
    }

    fn copy_selection(&self, cut: bool) {
        let Some(p) = self.current_pane() else { return };
        let uris = p.selected_uris();
        if uris.is_empty() {
            return;
        }
        clipboard::set(uris, cut);
        for_all_panes(|p| p.redraw());
    }

    fn paste_into(self: &Rc<Self>, dest: gio::File) {
        self.paste_with(dest, false);
    }

    /// Pastes into `dest`; `check`: each copy is read back and compared.
    fn paste_with(self: &Rc<Self>, dest: gio::File, check: bool) {
        let w = Rc::downgrade(self);
        clipboard::get(move |cut, uris| {
            let Some(s) = w.upgrade() else { return };
            if uris.is_empty() {
                s.flash_status(&tr("There is nothing on the clipboard to paste."));
                return;
            }
            let kind = if cut { Kind::Move } else { Kind::Copy };
            s.run_transfer_checked(kind, uris, dest.clone(), check);
            if cut {
                clipboard::clear_after_cut();
                for_all_panes(|p| p.redraw());
            }
        });
    }

    /// Run a copy/move/link and select the results if the destination is open.
    fn run_transfer(self: &Rc<Self>, kind: Kind, uris: Vec<String>, dest: gio::File) {
        self.run_transfer_checked(kind, uris, dest, false);
    }

    /// The same; `check`: each copy read back and compared with its original.
    fn run_transfer_checked(self: &Rc<Self>, kind: Kind, uris: Vec<String>, dest: gio::File, check: bool) {
        if dest.uri().starts_with("trash:") {
            if kind == Kind::Move {
                self.run_job(Job::Trash(uris));
            }
            return;
        }
        let items = uris
            .into_iter()
            .map(|u| Item { src: u, dest_dir: dest.uri().to_string(), name: None })
            .collect();
        let w = Rc::downgrade(self);
        let dest2 = dest.clone();
        ops::run(Some(self.window.clone().upcast()), Job::Transfer { kind, items, check }, move |created| {
            if let Some(s) = w.upgrade() {
                if let Some(p) = s.current_pane() {
                    if p.current().equal(&dest2) && !created.is_empty() {
                        let files: Vec<gio::File> = created.iter().map(|c| gio::File::for_uri(c)).collect();
                        p.add_files_now(&files);
                        p.select_uris(&created);
                    }
                }
            }
        });
    }

    fn run_job(self: &Rc<Self>, job: Job) {
        ops::run(Some(self.window.clone().upcast()), job, |_| {});
    }

    fn transfer_selection_here(self: &Rc<Self>, kind: Kind) {
        let Some(p) = self.current_pane() else { return };
        let uris = p.selected_uris();
        if !uris.is_empty() {
            self.run_transfer(kind, uris, p.current());
        }
    }

    fn transfer_to_chosen(self: &Rc<Self>, kind: Kind) {
        let Some(p) = self.current_pane() else { return };
        let uris = p.selected_uris();
        if uris.is_empty() {
            return;
        }
        let title = if kind == Kind::Move { tr("Move To") } else { tr("Copy To") };
        let d = gtk::FileChooserDialog::new(Some(&title), Some(&self.window), gtk::FileChooserAction::SelectFolder);
        d.add_button(&tr("_Cancel"), gtk::ResponseType::Cancel);
        d.add_button(&if kind == Kind::Move { tr("_Move") } else { tr("_Copy") }, gtk::ResponseType::Accept);
        d.set_default_response(gtk::ResponseType::Accept);
        let _ = d.set_current_folder_file(&p.current());
        let check = gtk::CheckButton::with_mnemonic(&tr("C_heck the copies"));
        check.set_tooltip_text(Some(&if kind == Kind::Move {
            tr("Each file moved to another drive is read back from there and compared with its original (CRC32), which is deleted only once they match. A move within one drive copies nothing, so there’s nothing to check.")
        } else {
            tr("Each copy is read back from where it went and compared with its original (CRC32). It takes about as long again as the copy.")
        }));
        d.set_extra_widget(&check);
        d.set_modal(true);
        let w = Rc::downgrade(self);
        d.connect_response(move |d, r| {
            if r == gtk::ResponseType::Accept {
                if let (Some(dest), Some(s)) = (d.file(), w.upgrade()) {
                    s.run_transfer_checked(kind, uris.clone(), dest, check.is_active());
                }
            }
            d.close();
        });
        d.show();
    }

    fn trash_selection(self: &Rc<Self>) {
        let Some(p) = self.current_pane() else { return };
        if p.is_trash() {
            self.delete_selection();
            return;
        }
        let uris = p.selected_uris();
        if uris.is_empty() {
            return;
        }
        // Where there's no trash (a phone, many network shares), ask once
        // whether to delete permanently, instead of a trash question and
        // then an error for every item.
        let can_trash = gio::File::for_uri(&uris[0])
            .query_info("access::can-trash", gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS, gio::Cancellable::NONE)
            .map(|i| !i.has_attribute("access::can-trash") || i.boolean("access::can-trash"))
            .unwrap_or(true);
        if !can_trash {
            let sel = p.selected();
            let q = if sel.len() == 1 {
                fill(&tr("“{name}” can’t be moved to the Trash. Delete it permanently?"), &[("name", &util::visible_controls(&sel[0].name))])
            } else {
                fill(&trn("This item can’t be moved to the Trash. Delete it permanently?", "These {n} items can’t be moved to the Trash. Delete them permanently?", sel.len() as u64), &[("n", &sel.len())])
            };
            if self.confirm(&q, &tr("This location has no trash. If you delete an item, it will be permanently lost."), &tr("_Delete"), true) {
                self.run_job(Job::Delete(uris));
            }
            return;
        }
        if config::with(|s| s.confirm_trash) {
            let sel = p.selected();
            let q = if sel.len() == 1 {
                fill(&tr("Move “{name}” to the Trash?"), &[("name", &util::visible_controls(&sel[0].name))])
            } else {
                fill(&trn("Move {n} item to the Trash?", "Move {n} items to the Trash?", sel.len() as u64), &[("n", &sel.len())])
            };
            if !self.confirm(&q, "", &tr("Move to _Trash"), true) {
                return;
            }
        }
        self.run_job(Job::Trash(uris));
    }

    fn delete_selection(self: &Rc<Self>) {
        let Some(p) = self.current_pane() else { return };
        let sel = p.selected();
        if sel.is_empty() {
            return;
        }
        let q = if sel.len() == 1 {
            fill(&tr("Are you sure you want to permanently delete “{name}”?"), &[("name", &util::visible_controls(&sel[0].name))])
        } else {
            fill(
                &trn("Are you sure you want to permanently delete the {n} selected item?", "Are you sure you want to permanently delete the {n} selected items?", sel.len() as u64),
                &[("n", &sel.len())],
            )
        };
        // Delete is the default, so Shift+Delete then Enter deletes.
        if self.confirm(&q, &tr("If you delete an item, it will be permanently lost."), &tr("_Delete"), true) {
            self.run_job(Job::Delete(sel.into_iter().map(|r| r.uri).collect()));
        }
    }

    fn restore_selection(self: &Rc<Self>) {
        let Some(p) = self.current_pane() else { return };
        let items: Vec<Item> = p
            .selected()
            .into_iter()
            .filter(|r| !r.orig.is_empty())
            .map(|r| Item {
                src: r.uri.clone(),
                dest_dir: gio::File::for_path(&r.orig).uri().to_string(),
                name: Some(r.name.clone()),
            })
            .collect();
        if !items.is_empty() {
            ops::run(Some(self.window.clone().upcast()), Job::Transfer { kind: Kind::Move, items, check: false }, |_| {});
        }
    }

    fn empty_trash(self: &Rc<Self>) {
        if self.confirm(
            &tr("Empty all items from the Trash?"),
            &tr("All items in the Trash will be permanently deleted."),
            &tr("_Empty Trash"),
            false,
        ) {
            self.run_job(Job::EmptyTrash);
        }
    }

    /// `ok_default`: Enter confirms (and the button has focus) instead of
    /// cancelling.
    fn confirm(&self, primary: &str, secondary: &str, ok: &str, ok_default: bool) -> bool {
        let d = gtk::MessageDialog::new(
            Some(&self.window),
            gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
            gtk::MessageType::Warning,
            gtk::ButtonsType::None,
            primary,
        );
        if !secondary.is_empty() {
            d.set_secondary_text(Some(secondary));
        }
        d.add_button(&tr("_Cancel"), gtk::ResponseType::Cancel);
        let ok_btn = d.add_button(ok, gtk::ResponseType::Ok);
        if ok_default {
            d.set_default_response(gtk::ResponseType::Ok);
            ok_btn.grab_focus();
        } else {
            d.set_default_response(gtk::ResponseType::Cancel);
        }
        let r = d.run();
        d.close();
        r == gtk::ResponseType::Ok
    }

    pub fn handle_drop(self: &Rc<Self>, uris: Vec<String>, dest: gio::File, action: gdk::DragAction) {
        // Ignore drops of items onto the folder they're already in.
        let all_here = uris.iter().all(|u| {
            gio::File::for_uri(u).parent().map(|p| p.equal(&dest)).unwrap_or(false)
        });
        if all_here && action != gdk::DragAction::LINK {
            return;
        }
        if action == gdk::DragAction::ASK {
            let menu = gtk::Menu::new();
            let choices = [
                (n_("_Move Here"), Some(Kind::Move), false),
                (n_("_Copy Here"), Some(Kind::Copy), false),
                (n_("_Link Here"), Some(Kind::Link), false),
                (n_("Move and C_heck Here"), Some(Kind::Move), true),
                (n_("Copy and Ch_eck Here"), Some(Kind::Copy), true),
                (n_("Cancel"), None, false),
            ];
            for (i, (label, kind, check)) in choices.into_iter().enumerate() {
                // Before the checked ones, and before Cancel.
                if i == 3 || i == 5 {
                    menu.append(&gtk::SeparatorMenuItem::new());
                }
                let item = gtk::MenuItem::with_mnemonic(&tr(label));
                let w = Rc::downgrade(self);
                let uris = uris.clone();
                let dest = dest.clone();
                item.connect_activate(move |_| {
                    if let (Some(s), Some(k)) = (w.upgrade(), kind) {
                        s.run_transfer_checked(k, uris.clone(), dest.clone(), check);
                    }
                });
                menu.append(&item);
            }
            menu.show_all();
            menu.set_attach_widget(Some(&self.window));
            menu.popup_at_pointer(None);
            return;
        }
        let kind = if action == gdk::DragAction::MOVE {
            Kind::Move
        } else if action == gdk::DragAction::LINK {
            Kind::Link
        } else {
            Kind::Copy
        };
        self.run_transfer(kind, uris, dest);
    }

    fn open_terminal(self: &Rc<Self>) {
        let Some(p) = self.current_pane() else { return };
        let dir = p
            .selected()
            .into_iter()
            .find(|r| r.is_dir)
            .map(|r| gio::File::for_uri(&r.uri))
            .unwrap_or_else(|| p.current());
        let Some(path) = dir.path() else { return };
        let Some(term) = util::terminal_command() else {
            util::error_dialog(Some(self.window.upcast_ref()), &tr("No terminal found"), &tr("Install mate-terminal or set $TERMINAL."));
            return;
        };
        let mut argv: Vec<std::ffi::OsString> = vec![term.clone().into()];
        if term.ends_with("mate-terminal") || term.ends_with("gnome-terminal") || term.ends_with("xfce4-terminal") {
            let mut flag = std::ffi::OsString::from("--working-directory=");
            flag.push(path.as_os_str());
            argv.push(flag);
        }
        if let Err(e) = util::spawn_in_dir(&argv, Some(&path)) {
            util::error_dialog(Some(self.window.upcast_ref()), &tr("Could not open a terminal"), &e);
        }
    }

    fn archive(self: &Rc<Self>, extract: bool) {
        let Some(p) = self.current_pane() else { return };
        let Some(tool) = util::archiver_command() else { return };
        let paths: Vec<std::ffi::OsString> = p.selected_files().iter().filter_map(|f| f.path()).map(|p| p.into_os_string()).collect();
        if paths.is_empty() {
            return;
        }
        let argv = util::archive_argv(&tool, extract, paths);
        if let Err(e) = util::spawn_in_dir(&argv, p.current().path().as_deref()) {
            util::error_dialog(Some(self.window.upcast_ref()), &tr("Could not start the archive manager"), &e);
        }
    }

    fn select_pattern_dialog(self: &Rc<Self>) {
        let d = gtk::Dialog::with_buttons(
            Some(&tr("Select Items Matching")),
            Some(&self.window),
            gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
            &[(tr("_Cancel").as_str(), gtk::ResponseType::Cancel), (tr("_Select").as_str(), gtk::ResponseType::Ok)],
        );
        d.set_default_response(gtk::ResponseType::Ok);
        let bx = d.content_area();
        bx.set_spacing(6);
        bx.set_border_width(12);
        bx.add(&gtk::Label::new(Some(&tr("Pattern (e.g. *.jpg, report-??.pdf):"))));
        let e = gtk::Entry::new();
        e.set_activates_default(true);
        e.set_text("*");
        bx.add(&e);
        d.show_all();
        let w = Rc::downgrade(self);
        d.connect_response(move |d, r| {
            if r == gtk::ResponseType::Ok {
                if let Some(p) = w.upgrade().and_then(|s| s.current_pane()) {
                    p.select_pattern(&e.text());
                }
            }
            d.close();
        });
    }

    fn edit_bookmarks(self: &Rc<Self>) {
        thread_local! {
            static OPEN: RefCell<glib::WeakRef<gtk::Dialog>> = RefCell::default();
        }
        if self.present_open(&OPEN.with(|o| o.borrow().upgrade())) {
            return;
        }
        let d = gtk::Dialog::with_buttons(
            Some(&tr("Edit Bookmarks")),
            Some(&self.window),
            gtk::DialogFlags::DESTROY_WITH_PARENT,
            &[(tr("_Close").as_str(), gtk::ResponseType::Close)],
        );
        d.set_default_size(460, 360);
        let bx = d.content_area();
        bx.set_border_width(8);
        bx.pack_start(&prefs::bookmark_editor(d.upcast_ref()), true, true, 0);
        d.connect_response(|d, _| d.close());
        OPEN.with(|o| o.borrow().set(Some(&d)));
        d.show_all();
    }

    /// Brings `open` (Preferences or Edit Bookmarks, shown once at a time,
    /// as in Caja) to the front of this window; false if it isn't open.
    fn present_open(&self, open: &Option<gtk::Dialog>) -> bool {
        let Some(d) = open else { return false };
        d.set_transient_for(Some(&self.window));
        // Its "win.*" check buttons act on the window it's in front of.
        d.insert_action_group("win", Some(&self.window));
        d.present();
        true
    }

    fn about(&self) {
        let d = gtk::AboutDialog::new();
        d.set_transient_for(Some(&self.window));
        d.set_modal(true);
        d.set_program_name("Arkha");
        // With the build (see build.rs), for bug reports: "1.0.0-dev (r142, 6a7d287)".
        let version = match env!("ARKHA_BUILD") {
            "" => env!("CARGO_PKG_VERSION").to_string(),
            build => format!("{} ({build})", env!("CARGO_PKG_VERSION")),
        };
        d.set_version(Some(&version));
        // GPLv3's "Appropriate Legal Notices": no warranty (here), and
        // how to see the license (its button).
        d.set_comments(Some(&format!("{}\n\n{}", tr("A fast graphical file manager."), tr("This program comes with absolutely no warranty."))));
        d.set_logo_icon_name(Some(util::app_icon()));
        // License, then the source code's repository (named so it fits
        // wherever that is), at the bottom left (as GTK places
        // its own Credits and License); Close stays at the right.
        const LICENSE: &str = "https://www.gnu.org/licenses/gpl-3.0.html";
        const REPOSITORY: &str = "https://github.com/dekart811/arkha";
        let links = [(tr("_License"), LICENSE, 1), (tr("_Source Code"), REPOSITORY, 2)];
        for (label, _, id) in &links {
            let b = d.add_button(label, gtk::ResponseType::Other(*id));
            if let Some(bar) = b.parent().and_downcast::<gtk::ButtonBox>() {
                bar.set_child_secondary(&b, true);
            }
        }
        // GTK makes all its texts selectable: only the version is worth
        // copying (for a bug report).
        fn unselectable(w: &gtk::Widget, keep: &str) {
            if let Some(l) = w.downcast_ref::<gtk::Label>() {
                if l.text() != keep {
                    l.set_selectable(false);
                }
            } else if let Some(c) = w.downcast_ref::<gtk::Container>() {
                for child in c.children() {
                    unselectable(&child, keep);
                }
            }
        }
        unselectable(d.content_area().upcast_ref(), &version);
        d.connect_response(move |d, r| {
            let gtk::ResponseType::Other(id) = r else {
                d.close();
                return;
            };
            if let Some((_, uri, _)) = links.iter().find(|(_, _, i)| *i == id) {
                if let Err(e) = gtk::show_uri_on_window(Some(d), uri, gtk::current_event_time()) {
                    util::error_dialog(Some(d.upcast_ref()), &fill(&tr("Could not open “{name}”"), &[("name", uri)]), e.message());
                }
            }
        });
        d.show();
    }

    /// Help → Manual: its first page in the web browser (the default
    /// application for HTML files).
    fn manual(&self) {
        let window = self.window.upcast_ref::<gtk::Window>();
        // Not gtk::show_uri_on_window: on Wayland it hands the window to a
        // desktop portal, warning where there's none.
        let open = |window: &gtk::Window, uri: &str| {
            let ctx = window.display().app_launch_context();
            if let Some(c) = &ctx {
                c.set_timestamp(gtk::current_event_time());
            }
            if let Err(e) = gio::AppInfo::launch_default_for_uri(uri, ctx.as_ref()) {
                util::error_dialog(Some(window), &tr("Could not open the manual"), e.message());
            }
        };
        match crate::i18n::manual() {
            Ok(page) => open(window, &gio::File::for_path(&page).uri()),
            // Missing from the package (an AUR helper that built with an old
            // PKGBUILD left it out): this version's pages online instead.
            Err(looked) => {
                let places: Vec<String> = looked.iter().map(|d| d.display().to_string()).collect();
                let d = gtk::MessageDialog::new(
                    Some(window),
                    gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
                    gtk::MessageType::Error,
                    gtk::ButtonsType::None,
                    &tr("The manual isn’t installed"),
                );
                let found = fill(&tr("It wasn’t found in:\n{places}"), &[("places", &places.join("\n"))]);
                d.set_secondary_text(Some(&format!("{found}\n\n{}", tr("You can read this version’s manual online instead."))));
                d.add_button(&tr("_Close"), gtk::ResponseType::Close);
                d.add_button(&tr("Read It _Online"), gtk::ResponseType::Accept);
                d.set_default_response(gtk::ResponseType::Accept);
                d.connect_response(move |d, r| {
                    d.close();
                    if r == gtk::ResponseType::Accept {
                        open(d.transient_for().as_ref().unwrap_or(d.upcast_ref()), &crate::i18n::online_manual());
                    }
                });
                d.show();
            }
        }
    }

    fn shortcuts(&self) {
        let d = gtk::Dialog::with_buttons(Some(&tr("Keyboard Shortcuts")), Some(&self.window), gtk::DialogFlags::DESTROY_WITH_PARENT, &[(tr("_Close").as_str(), gtk::ResponseType::Close)]);
        d.set_default_size(-1, 560);
        let grid = gtk::Grid::new();
        grid.set_row_spacing(4);
        grid.set_column_spacing(24);
        grid.set_border_width(14);
        let mut r = 0;
        let mut add = |k: &str, v: &str| {
            let kl = gtk::Label::new(None);
            if v.is_empty() {
                kl.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(k)));
                kl.set_margin_top(if r == 0 { 0 } else { 8 });
            } else {
                kl.set_text(k);
            }
            kl.set_xalign(0.0);
            grid.attach(&kl, 0, r, 1, 1);
            let vl = gtk::Label::new(Some(v));
            vl.set_xalign(0.0);
            vl.style_context().add_class("dim-label");
            grid.attach(&vl, 1, r, 1, 1);
            r += 1;
        };
        // The current shortcuts (they can be changed in Preferences).
        let mut group = "";
        for b in keys::BINDINGS {
            let current = keys::current(b.action);
            if current.is_empty() {
                continue;
            }
            if b.group != group {
                group = b.group;
                add(&tr(group), "");
            }
            add(&keys::labels(&current), &tr(b.label));
        }
        add(&tr("Mouse"), "");
        add(&tr("Middle-click a folder"), &tr("Open in new tab"));
        add(&tr("Drag with Ctrl / Shift / Alt"), &tr("Copy / Move / Ask"));
        add(&tr("Ctrl+scroll"), &tr("Zoom"));
        add(&tr("Back / forward mouse buttons"), &tr("Back / Forward"));
        let sw = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        sw.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        sw.set_propagate_natural_width(true);
        sw.add(&grid);
        d.content_area().pack_start(&sw, true, true, 0);
        d.connect_response(|d, _| d.close());
        d.show_all();
    }
}

thread_local! {
    static SIDEBAR_CSS: gtk::CssProvider = {
        let css = gtk::CssProvider::new();
        if let Some(screen) = gdk::Screen::default() {
            gtk::StyleContext::add_provider_for_screen(&screen, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
        css
    };
}

/// Side pane row height from the settings. The rows are separate widgets, so
/// this needs a screen-wide provider; the class keeps it off file choosers.
fn apply_sidebar_css() {
    let h = config::with(|s| s.sidebar_row_height);
    // The eject buttons are no taller than their icon, so device rows can be
    // as low as the others.
    let mut data = String::from("list.arkha-sidebar button.sidebar-button { min-height: 0; min-width: 0; padding: 2px; margin: 0; }");
    if h > 0 {
        data.push_str(&format!(" list.arkha-sidebar row.sidebar-row {{ min-height: {h}px; }}"));
    }
    SIDEBAR_CSS.with(|css| {
        let _ = css.load_from_data(data.as_bytes());
    });
}

#[derive(Clone, Copy)]
enum OpenMode {
    Normal,
    Tab,
    Window,
}

fn location_text(f: &gio::File) -> String {
    match f.path() {
        Some(p) => p.to_string_lossy().to_string(),
        None => f.uri().to_string(),
    }
}

/// What completing `base` adds, with the folders `dirs` there: the rest of
/// the only name starting with it (with a "/"), or of the part all the
/// names starting with it have in common. Case matters; hidden folders
/// only once a dot is typed.
fn completion_suffix(base: &str, dirs: &[String]) -> Option<String> {
    let mut common: Option<String> = None;
    for n in dirs.iter().filter(|n| n.starts_with(base) && (base.starts_with('.') || !n.starts_with('.'))) {
        let full = format!("{n}/");
        common = Some(match common {
            None => full,
            Some(c) => c.chars().zip(full.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a).collect(),
        });
    }
    common.and_then(|c| c.get(base.len()..).filter(|rest| !rest.is_empty()).map(str::to_string))
}

fn parse_location(text: &str, base: Option<gio::File>) -> gio::File {
    // Up to a NUL, as a text field would have it: paths can't hold one.
    let text = text.split('\0').next().unwrap_or_default();
    // "~" and "~/…" are in the home folder; "~notes" is a name like any other.
    if text == "~" || text.starts_with("~/") {
        return gio::File::for_path(format!("{}{}", glib::home_dir().to_string_lossy(), &text[1..]));
    }
    if text.starts_with('/') {
        return gio::File::for_path(text);
    }
    if text.contains("://") || text.starts_with("trash:") {
        return gio::File::for_uri(text);
    }
    match base {
        Some(b) => b.resolve_relative_path(text),
        None => gio::File::for_path(text),
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_pane_opens_where_preferences_say() {
        let t = crate::testutil::TestDir::new("split-opens");
        let (here, there) = (gio::File::for_path(t.0.join("here")), t.0.join("there"));
        std::fs::create_dir_all(&there).unwrap();
        let there_uri = gio::File::for_path(&there).uri().to_string();
        let gone = gio::File::for_path(t.0.join("gone")).uri().to_string();
        let at = |opens: &str, folder: &str, last: &str| split_opens_at(opens, &here, folder, last).uri().to_string();
        assert_eq!(at("same", &there_uri, &there_uri), here.uri());
        assert_eq!(at("home", "", ""), util::home_file().uri());
        assert_eq!(at("last", "", &there_uri), there_uri);
        assert_eq!(at("folder", &there_uri, ""), there_uri);
        assert_eq!(at("folder", "", ""), here.uri(), "no folder chosen: the same as the pane in use");
        assert_eq!(at("last", "", &gone), here.uri(), "a local folder that's gone");
        assert_eq!(at("folder", "smb://nas/share", ""), "smb://nas/share/", "a network place isn't looked up first");
        assert_eq!(at("nonsense", &there_uri, ""), here.uri());
    }

    /// Tabs as a list of names: opens `name` from the current tab (or as a
    /// new tab), as AppWindow::insert_tab places it.
    struct Tabs {
        mode: &'static str,
        tabs: Vec<&'static str>,
        current: usize,
        last_opened: Option<&'static str>,
    }

    impl Tabs {
        fn open(&mut self, name: &'static str, from_current: bool) {
            let last = self.last_opened.and_then(|l| self.tabs.iter().position(|t| *t == l)).map(|i| i as u32);
            match tab_index(self.mode, from_current, Some(self.current as u32), last) {
                Some(i) => self.tabs.insert(i as usize, name),
                None => self.tabs.push(name),
            }
            if from_current {
                self.last_opened = Some(name);
            }
        }
        fn choose(&mut self, name: &str) {
            self.current = self.tabs.iter().position(|t| *t == name).unwrap();
            self.last_opened = None;
        }
    }

    #[test]
    fn tabs_open_where_firefox_puts_them() {
        let mut t = Tabs { mode: "in-order", tabs: vec!["a"], current: 0, last_opened: None };
        t.open("b", true);
        t.open("c", true);
        t.open("d", true);
        assert_eq!(t.tabs, ["a", "b", "c", "d"]);
        // Ctrl+T: at the end; the next opened from "a" still follows "d".
        t.open("new", false);
        t.open("e", true);
        assert_eq!(t.tabs, ["a", "b", "c", "d", "e", "new"]);
        // Another tab chosen: right after it, then after that one.
        t.choose("b");
        t.open("b1", true);
        t.open("b2", true);
        assert_eq!(t.tabs, ["a", "b", "b1", "b2", "c", "d", "e", "new"]);
        // Back to "a": starts over right after it.
        t.choose("a");
        t.open("a1", true);
        assert_eq!(t.tabs[..3], ["a", "a1", "b"]);
    }

    #[test]
    fn tabs_open_after_the_current_one_or_at_the_end() {
        let mut t = Tabs { mode: "after", tabs: vec!["a", "z"], current: 0, last_opened: None };
        t.open("b", true);
        t.open("c", true);
        t.open("new", false);
        assert_eq!(t.tabs, ["a", "new", "c", "b", "z"]);
        let mut t = Tabs { mode: "end", tabs: vec!["a", "z"], current: 0, last_opened: None };
        t.open("b", true);
        t.open("new", false);
        assert_eq!(t.tabs, ["a", "z", "b", "new"]);
        // No current tab: at the end.
        assert_eq!(tab_index("in-order", true, None, None), None);
        assert_eq!(tab_index("after", false, None, None), None);
    }

    #[test]
    fn location_completion() {
        let dirs: Vec<String> = [".ssh", ".cache", ".cargo", "Desktop", "Documents", "Downloads", "Projects", "Projects-old", "Ünï"]
            .map(String::from)
            .to_vec();
        let c = |base: &str| completion_suffix(base, &dirs);
        assert_eq!(c(".ss").as_deref(), Some("h/"));
        assert_eq!(c(".car").as_deref(), Some("go/"));
        assert_eq!(c("Pro").as_deref(), Some("jects"));
        assert_eq!(c("Projects-").as_deref(), Some("old/"));
        assert_eq!(c("Ü").as_deref(), Some("nï/"));
        assert_eq!(c("Projects/"), None);
        // Several names, nothing more in common.
        assert_eq!(c("D"), None);
        assert_eq!(c("Do"), None);
        assert_eq!(c(".ca"), None);
        // Case matters, as in Caja.
        assert_eq!(c("doc"), None);
        assert_eq!(c("x"), None);
        // Hidden folders only once a dot is typed.
        let one: Vec<String> = vec![".hidden".into(), "shown".into()];
        assert_eq!(completion_suffix("s", &one).as_deref(), Some("hown/"));
        assert_eq!(completion_suffix(".", &one).as_deref(), Some("hidden/"));
        // Already complete.
        assert_eq!(c("Downloads/"), None);
    }

    #[test]
    fn typed_locations() {
        let home = glib::home_dir();
        let base = || Some(gio::File::for_path("/srv/data"));
        assert_eq!(parse_location("~", None).path(), Some(home.clone()));
        assert_eq!(parse_location("~/Music", None).path(), Some(home.join("Music")));
        assert_eq!(parse_location("/etc", base()).path(), Some("/etc".into()));
        assert_eq!(parse_location("sub/dir", base()).path(), Some("/srv/data/sub/dir".into()));
        assert_eq!(parse_location("../x", base()).path(), Some("/srv/x".into()));
        // A name starting with "~" in the current folder.
        assert_eq!(parse_location("~notes", base()).path(), Some("/srv/data/~notes".into()));
        assert_eq!(parse_location("trash:///", None).uri(), "trash:///");
        assert_eq!(parse_location("sftp://host/srv", None).uri(), "sftp://host/srv");
        // Shown as a path for local files, else as the URI.
        assert_eq!(location_text(&gio::File::for_path("/a b")), "/a b");
        assert_eq!(location_text(&gio::File::for_uri("trash:///")), "trash:///");
    }

    #[test]
    fn random_typed_locations() {
        crate::testutil::cases(2000, |r| {
            let text = r.text(20);
            let base = r.chance(2).then(|| gio::File::for_path("/srv/data"));
            let _ = parse_location(&text, base);
            let dirs: Vec<String> = (0..r.below(6)).map(|_| r.text(8)).collect();
            // What's added leads to one of the folders.
            if let Some(more) = completion_suffix(&text, &dirs) {
                assert!(dirs.iter().any(|d| format!("{d}/").starts_with(&format!("{text}{more}"))), "{text:?} + {more:?} in {dirs:?}");
            }
        });
    }
}
