//! The menu bar and the context menus.

use super::*;

/// Arrange Items' sort orders: the name and the first three details (the
/// usual ones), or (`more`) the others there are menu items for.
fn sort_section(more: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    if !more {
        menu.append(Some(&tr("By _Name")), Some("win.sort-by::name"));
    }
    for (i, d) in crate::details::DETAILS.iter().enumerate() {
        if let Some(label) = d.menu.filter(|_| (i >= 3) == more) {
            menu.append(Some(&tr(label)), Some(&format!("win.sort-by::{}", d.id)));
        }
    }
    menu
}

impl AppWindow {
    pub(super) fn build_menubar(&self, bar: &gio::Menu) {
        let section = |items: &[(&str, &str)]| {
            let m = gio::Menu::new();
            for (label, action) in items {
                m.append(Some(&tr(label)), Some(action));
            }
            m
        };
        let file = gio::Menu::new();
        file.append_section(None, &section(&[(n_("New _Tab"), "win.new-tab"), (n_("New _Window"), "win.new-window")]));
        let create = gio::Menu::new();
        create.append(Some(&tr("Create _Folder")), Some("win.new-folder"));
        create.append_submenu(Some(&tr("Create _Document")), &self.templates_menu);
        file.append_section(None, &create);
        file.append_section(None, &section(&[(n_("Connect to _Server…"), "win.connect-server")]));
        file.append_section(None, &section(&[(n_("Open in _Terminal"), "win.terminal"), (n_("_Properties"), "win.properties")]));
        file.append_section(None, &section(&[(n_("_Empty Trash"), "win.empty-trash")]));
        file.append_section(None, &section(&[(n_("Move Tab _Left"), "win.tab-left"), (n_("Move Tab Ri_ght"), "win.tab-right")]));
        file.append_section(None, &section(&[(n_("_Close Tab"), "win.close-tab"), (n_("_Reopen Closed Tab"), "win.reopen-tab"), (n_("Close _Window"), "win.close-window"), (n_("Close _All Windows"), "win.close-all-windows"), (n_("_Quit"), "app.quit")]));
        bar.append_submenu(Some(&tr("_File")), &file);

        let edit = gio::Menu::new();
        self.undo_menu.append(Some(&tr("_Undo")), Some("win.undo"));
        self.undo_menu.append(Some(&tr("_Redo")), Some("win.redo"));
        edit.append_section(None, &self.undo_menu);
        edit.append_section(None, &section(&[(n_("Cu_t"), "win.cut"), (n_("_Copy"), "win.copy"), (n_("_Paste"), "win.paste"), (n_("Paste and C_heck"), "win.paste-check"), (n_("Paste _Into Folder"), "win.paste-into")]));
        edit.append_section(None, &section(&[(n_("Select _All"), "win.select-all"), (n_("Select I_tems Matching…"), "win.select-pattern"), (n_("_Invert Selection"), "win.invert-selection")]));
        edit.append_section(None, &section(&[(n_("Dup_licate"), "win.duplicate"), (n_("Ma_ke Link"), "win.make-link"), (n_("_Rename…"), "win.rename"), (n_("Copy T_o…"), "win.copy-to"), (n_("_Move To…"), "win.move-to")]));
        edit.append_section(None, &section(&[(n_("Copy to Other _Pane"), "win.copy-to-pane"), (n_("Move to Other Pa_ne"), "win.move-to-pane")]));
        edit.append_section(None, &section(&[(n_("Mo_ve to Trash"), "win.trash"), (n_("_Delete"), "win.delete"), (n_("_Restore"), "win.restore")]));
        edit.append_section(None, &section(&[(n_("_Backgrounds and Emblems…"), "win.backgrounds"), (n_("Pr_eferences"), "win.preferences")]));
        bar.append_submenu(Some(&tr("_Edit")), &edit);

        let view = gio::Menu::new();
        view.append_section(None, &section(&[(n_("_Reload"), "win.reload"), (n_("Sto_p"), "win.stop")]));
        view.append_section(None, &section(&[(n_("_Icons"), "win.view-mode::icon"), (n_("_List"), "win.view-mode::list"), (n_("_Compact"), "win.view-mode::compact"), (n_("C_olumns"), "win.view-mode::columns")]));
        view.append_section(None, &section(&[(n_("Show _Hidden Files"), "win.show-hidden"), (n_("_Side Pane"), "win.show-sidebar"), (n_("E_xtra Pane"), "win.split-view"), (n_("St_atusbar"), "win.show-statusbar"), (n_("_Info Panel"), "win.info-panel")]));
        view.append_section(None, &section(&[(n_("Swap Pa_nes"), "win.swap-panes"), (n_("S_witch to Other Pane"), "win.switch-pane"), (n_("Same Location as O_ther Pane"), "win.same-location")]));
        view.append_section(None, &section(&[(n_("_Main Toolbar"), "win.show-toolbar"), (n_("Location _Bar"), "win.show-locbar")]));
        let sort = gio::Menu::new();
        sort.append_section(None, &sort_section(false));
        sort.append_section(None, &sort_section(true));
        sort.append_section(None, &section(&[(n_("_Reversed Order"), "win.sort-desc"), (n_("_Folders Before Files"), "win.folders-first")]));
        view.append_submenu(Some(&tr("Arrange _Items")), &sort);
        view.append_section(None, &section(&[(n_("_Visible Columns…"), "win.visible-columns")]));
        view.append_section(None, &section(&[(n_("Reset View to _Defaults"), "win.reset-view")]));
        view.append_section(None, &section(&[(n_("Zoom _In"), "win.zoom-in"), (n_("Zoom _Out"), "win.zoom-out"), (n_("Normal Si_ze"), "win.zoom-normal")]));
        view.append_section(None, &section(&[(n_("Loc_k Toolbar"), "win.lock-bar")]));
        bar.append_submenu(Some(&tr("_View")), &view);

        let go = gio::Menu::new();
        go.append_section(None, &section(&[(n_("_Back"), "win.back"), (n_("_Forward"), "win.forward"), (n_("_Open Parent"), "win.up")]));
        go.append_section(None, &section(&[(n_("_Home Folder"), "win.home"), (n_("_Desktop"), "win.go-desktop"), (n_("File _System"), "win.go-root"), (n_("_Trash"), "win.go-trash")]));
        go.append_section(None, &section(&[(n_("_Location…"), "win.location"), (n_("_Filter…"), "win.filter")]));
        go.append_section(None, &section(&[(n_("Clea_r History"), "win.clear-history")]));
        bar.append_submenu(Some(&tr("_Go")), &go);

        let bm = gio::Menu::new();
        bm.append_section(None, &section(&[(n_("_Add Bookmark"), "win.add-bookmark"), (n_("_Edit Bookmarks…"), "win.edit-bookmarks")]));
        bm.append_section(None, &self.bookmarks_menu);
        bar.append_submenu(Some(&tr("_Bookmarks")), &bm);

        let help = gio::Menu::new();
        help.append(Some(&tr("_Manual")), Some("win.manual"));
        help.append(Some(&tr("_Keyboard Shortcuts")), Some("win.shortcuts"));
        help.append(Some(&tr("_About")), Some("win.about"));
        bar.append_submenu(Some(&tr("_Help")), &help);
    }

    pub(super) fn refresh_bookmarks(&self) {
        self.bookmarks_menu.remove_all();
        for (uri, label) in util::read_bookmarks() {
            let item = gio::MenuItem::new(Some(&label.replace('_', "__")), None);
            item.set_action_and_target_value(Some("win.go-uri"), Some(&uri.to_variant()));
            self.bookmarks_menu.append_item(&item);
        }
        self.sidebar.refresh();
    }

    pub(super) fn refresh_templates(&self) {
        self.templates_menu.remove_all();
        let empty = gio::Menu::new();
        empty.append(Some(&tr("_Empty Document")), Some("win.new-file"));
        self.templates_menu.append_section(None, &empty);
        let tm = gio::Menu::new();
        if let Some(dir) = glib::user_special_dir(glib::UserDirectory::Templates) {
            if dir != glib::home_dir() {
                if let Ok(rd) = std::fs::read_dir(&dir) {
                    let mut names: Vec<_> = rd.flatten().filter(|e| e.path().is_file()).map(|e| e.path()).collect();
                    names.sort();
                    for p in names {
                        let label = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        let item = gio::MenuItem::new(Some(&label.replace('_', "__")), None);
                        item.set_action_and_target_value(Some("win.new-from-template"), Some(&p.to_string_lossy().to_string().to_variant()));
                        tm.append_item(&item);
                    }
                }
            }
        }
        self.templates_menu.append_section(None, &tm);
    }

    pub(super) fn open_with_menu(&self, sel: &[crate::pane::RowInfo]) -> gio::Menu {
        let m = gio::Menu::new();
        if let Some(first) = sel.first() {
            let same_type = sel.iter().all(|r| r.ctype == first.ctype);
            if same_type {
                let apps = gio::AppInfo::all_for_type(&first.ctype);
                let default = gio::AppInfo::default_for_type(&first.ctype, false);
                let mut seen = std::collections::HashSet::new();
                for app in default.into_iter().chain(apps) {
                    let Some(id) = app.id() else { continue };
                    if !seen.insert(id.to_string()) {
                        continue;
                    }
                    let item = gio::MenuItem::new(Some(&app.display_name().replace('_', "__")), None);
                    item.set_action_and_target_value(Some("win.open-with"), Some(&id.to_string().to_variant()));
                    m.append_item(&item);
                }
            }
        }
        let other = gio::Menu::new();
        other.append(Some(&tr("Other _Application…")), Some("win.open-with-other"));
        m.append_section(None, &other);
        m
    }

    pub(super) fn context_menu(self: &Rc<Self>, pane: &Rc<Pane>, event: Option<gdk::Event>, on_item: bool) {
        let menu = gio::Menu::new();
        let cfg = config::get();
        let shown = |key: &str| !cfg.hidden_menu_items.iter().any(|h| h == key);
        let sec = |items: &[(&str, &str, &str)]| {
            let m = gio::Menu::new();
            for (k, l, a) in items {
                if shown(k) {
                    m.append(Some(&tr(l)), Some(a));
                }
            }
            m
        };
        let sel = pane.selected();
        let in_trash = pane.is_trash();
        let on_sel = on_item && !sel.is_empty();
        // Every entry that applies here, by key; the order set in the
        // preferences (actions::menu_order) then lays them out.
        let mut entries: std::collections::HashMap<String, gio::MenuItem> = Default::default();
        let mut add = |k: &str, label: &str, action: &str| {
            entries.insert(k.to_string(), gio::MenuItem::new(Some(&tr(label)), Some(action)));
        };
        let mut submenus: Vec<(&str, &str, gio::Menu)> = vec![];
        if on_sel {
            let all_dirs = sel.iter().all(|r| r.is_dir);
            if in_trash {
                add("restore", n_("_Restore"), "win.restore");
                add("cut", n_("Cu_t"), "win.cut");
                add("copy", n_("_Copy"), "win.copy");
                add("delete", n_("_Delete Permanently"), "win.delete");
            } else {
                if all_dirs {
                    add("open", n_("_Open"), "win.open");
                    add("open-new-tab", n_("Open in New _Tab"), "win.open-new-tab");
                    add("open-new-window", n_("Open in New _Window"), "win.open-new-window");
                } else {
                    let label = gio::AppInfo::default_for_type(&sel[0].ctype, false)
                        .filter(|_| sel.iter().all(|r| r.ctype == sel[0].ctype))
                        .map(|a| fill(&tr("_Open With {program}"), &[("program", &a.display_name().replace('_', "__"))]))
                        .unwrap_or_else(|| n_("_Open").into());
                    add("open", &label, "win.open");
                }
                // Caja's entry in search results.
                if pane.is_searching() && sel.len() == 1 {
                    add("open-parent", n_("Open parent location"), "win.open-parent");
                }
                if shown("open-with") {
                    submenus.push(("open-with", n_("Open Wit_h"), self.open_with_menu(&sel)));
                }
                add("cut", n_("Cu_t"), "win.cut");
                add("copy", n_("_Copy"), "win.copy");
                if sel.len() == 1 && all_dirs {
                    add("paste-into", n_("Paste _Into Folder"), "win.paste-into");
                }
                add("duplicate", n_("D_uplicate"), "win.duplicate");
                add("make-link", n_("Ma_ke Link"), "win.make-link");
                add("rename", n_("_Rename…"), "win.rename");
                // The bulk rename window: "Rename All…" for several items
                // (as in Caja's caja-rename), "Advanced Rename…" for one.
                add("rename-all", if sel.len() > 1 { n_("Rename _All…") } else { n_("Advanced Re_name…") }, "win.rename-all");
                add("copy-to", n_("Copy T_o…"), "win.copy-to");
                add("move-to", n_("_Move To…"), "win.move-to");
                add("trash", n_("Mo_ve to Trash"), "win.trash");
                add("delete", n_("_Delete"), "win.delete");
                if util::archiver_command().is_some() {
                    if sel.iter().all(|r| util::is_archive(&r.ctype)) {
                        add("extract", n_("E_xtract Here"), "win.extract");
                    }
                    add("compress", n_("_Compress…"), "win.compress");
                }
                if sel.len() == 1 && all_dirs {
                    add("terminal", n_("Open in _Terminal"), "win.terminal");
                }
            }
            add("properties", n_("_Properties"), "win.properties");
        } else {
            if in_trash {
                add("bg:empty-trash", n_("_Empty Trash"), "win.empty-trash");
            } else {
                add("bg:new-folder", n_("Create _Folder"), "win.new-folder");
                if shown("bg:new-document") {
                    submenus.push(("bg:new-document", n_("Create _Document"), self.templates_menu.clone()));
                }
                add("bg:terminal", n_("Open in _Terminal"), "win.terminal");
                add("bg:paste", n_("_Paste"), "win.paste");
                add("bg:paste-check", n_("Paste and C_heck"), "win.paste-check");
                // The menu bar's View menu without Reload, zooming and
                // Arrange Items (their own entries); each entry can be hidden.
                if shown("bg:view") {
                    let sub = gio::Menu::new();
                    sub.append_section(None, &sec(&[("bg:view-icons", n_("_Icons"), "win.view-mode::icon"), ("bg:view-list", n_("_List"), "win.view-mode::list"), ("bg:view-compact", n_("_Compact"), "win.view-mode::compact"), ("bg:view-columns", n_("C_olumns"), "win.view-mode::columns")]));
                    sub.append_section(None, &sec(&[("bg:show-hidden", n_("Show _Hidden Files"), "win.show-hidden"), ("bg:view-sidepane", n_("_Side Pane"), "win.show-sidebar"), ("bg:view-extrapane", n_("E_xtra Pane"), "win.split-view"), ("bg:view-statusbar", n_("St_atusbar"), "win.show-statusbar")]));
                    // Left out if every entry in it is hidden.
                    if (0..sub.n_items()).any(|i| sub.item_link(i, "section").is_some_and(|m| m.n_items() > 0)) {
                        submenus.push(("bg:view", n_("_View"), sub));
                    }
                }
                if shown("bg:arrange") {
                    let arrange = gio::Menu::new();
                    arrange.append_section(None, &sort_section(false));
                    arrange.append_section(None, &sort_section(true));
                    let more = gio::Menu::new();
                    more.append(Some(&tr("_Reversed Order")), Some("win.sort-desc"));
                    more.append(Some(&tr("_Folders Before Files")), Some("win.folders-first"));
                    arrange.append_section(None, &more);
                    submenus.push(("bg:arrange", n_("Arrange _Items"), arrange));
                }
                add("bg:add-bookmark", n_("Add _Bookmark"), "win.add-bookmark");
            }
            add("bg:select-all", n_("Select _All"), "win.select-all");
            add("bg:reload", n_("_Reload"), "win.reload");
            add("bg:properties", n_("_Properties"), "win.properties");
        }
        for (k, label, sub) in submenus {
            entries.insert(k.to_string(), gio::MenuItem::new_submenu(Some(&tr(label)), &sub));
        }
        // Custom actions, only for local folders: their commands get paths.
        let custom = actions::load();
        if !in_trash && pane.current().path().is_some() && sel.iter().all(|r| !on_sel || gio::File::for_uri(&r.uri).path().is_some()) {
            let picked: Vec<(String, bool, String)> = if on_sel { sel.iter().map(|r| (r.name.clone(), r.is_dir, r.ctype.clone())).collect() } else { vec![] };
            for (i, a) in custom.iter().enumerate() {
                if actions::applies(a, &picked) {
                    let target = format!("{i}{}", if on_sel { "" } else { "/bg" });
                    let item = gio::MenuItem::new(Some(&a.name.replace('_', "__")), None);
                    item.set_action_and_target_value(Some("win.custom-action"), Some(&target.to_variant()));
                    entries.insert(actions::custom_key(a), item);
                }
            }
        }
        let order = if on_sel {
            let keys: Vec<String> = custom.iter().filter(|a| a.files || a.folders).map(actions::custom_key).collect();
            actions::menu_order(&cfg.menu_order, actions::ITEM_ORDER, &keys)
        } else {
            let keys: Vec<String> = custom.iter().filter(|a| a.background).map(actions::custom_key).collect();
            actions::menu_order(&cfg.menu_order_bg, actions::BACKGROUND_ORDER, &keys)
        };
        // Sections between separators; empty ones vanish, so separators
        // around entries that don't apply here never double up.
        let mut section = gio::Menu::new();
        for k in &order {
            if k == actions::SEPARATOR {
                if section.n_items() > 0 {
                    menu.append_section(None, &section);
                    section = gio::Menu::new();
                }
            } else if shown(k) {
                if let Some(item) = entries.remove(k) {
                    section.append_item(&item);
                }
            }
        }
        if section.n_items() > 0 {
            menu.append_section(None, &section);
        }
        self.update_action_states();
        let m = gtk::Menu::from_model(&menu);
        m.set_attach_widget(Some(&pane.active_view()));
        m.show_all();
        match event {
            // A right-click in a column of the columns view that's gone
            // since (it led to another folder): at the pointer, in the view.
            Some(e) if e.window().is_some_and(|w| w.is_destroyed()) => {
                let view = pane.active_view();
                let pointer = view.display().default_seat().and_then(|s| s.pointer());
                match (view.window(), pointer) {
                    (Some(win), Some(ptr)) if e.event_type() == gdk::EventType::ButtonPress => {
                        // The same click, moved to the view (GTK wants the
                        // click that opened a menu).
                        let (_, x, y, _) = win.device_position(&ptr);
                        let mut click = e.clone();
                        unsafe {
                            let b = glib::translate::ToGlibPtrMut::<*mut gdk::ffi::GdkEvent>::to_glib_none_mut(&mut click).0 as *mut gdk::ffi::GdkEventButton;
                            if !(*b).window.is_null() {
                                glib::gobject_ffi::g_object_unref((*b).window as *mut _);
                            }
                            (*b).window = glib::translate::ToGlibPtr::<*mut gdk::ffi::GdkWindow>::to_glib_full(&win);
                            (*b).x = x as f64;
                            (*b).y = y as f64;
                        }
                        m.popup_at_pointer(Some(&click));
                    }
                    _ => m.popup_at_widget(&view, gdk::Gravity::Center, gdk::Gravity::NorthWest, None),
                }
            }
            Some(e) => m.popup_at_pointer(Some(&e)),
            None => m.popup_at_widget(&pane.active_view(), gdk::Gravity::Center, gdk::Gravity::NorthWest, None),
        }
    }

    /// Runs custom action `target` ("index", or "index/bg" for the menu of the
    /// folder's empty space) on the current tab's selection.
    pub(super) fn run_custom_action(self: &Rc<Self>, target: &str) {
        let (idx, bg) = match target.strip_suffix("/bg") {
            Some(i) => (i, true),
            None => (target, false),
        };
        let (Some(a), Some(p)) = (idx.parse::<usize>().ok().and_then(|i| actions::load().into_iter().nth(i)), self.current_pane()) else { return };
        let Some(dir) = p.current().path() else { return };
        let sel: Vec<std::path::PathBuf> = if bg { vec![] } else { p.selected_files().iter().filter_map(|f| f.path()).collect() };
        let cmd = actions::expand(&a.command, &sel, &dir);
        if let Err(e) = util::spawn_in_dir(&[actions::shell().into(), std::ffi::OsString::from("-c"), cmd], Some(&dir)) {
            util::error_dialog(Some(self.window.upcast_ref()), &fill(&tr("Could not run “{name}”"), &[("name", &a.name)]), &e);
        }
    }
}
