//! File Manager Preferences: its pages, and the editors for the lists and
//! orders it holds (menus, side pane places, bookmarks, shortcuts, custom
//! thumbnailers and actions).

use super::*;

impl AppWindow {
    pub(super) fn preferences(self: &Rc<Self>) {
        thread_local! {
            static OPEN: RefCell<glib::WeakRef<gtk::Dialog>> = RefCell::default();
        }
        if self.present_open(&OPEN.with(|o| o.borrow().upgrade())) {
            return;
        }
        let s = config::get();
        let d = gtk::Dialog::new();
        OPEN.with(|o| o.borrow().set(Some(&d)));
        d.set_title(&tr("File Manager Preferences"));
        d.set_transient_for(Some(&self.window));
        d.set_destroy_with_parent(true);
        d.set_default_size(680, 480);
        // Dialogs don't see the window's actions, which the "win.*" check buttons use.
        d.insert_action_group("win", Some(&self.window));

        // Nemo-style layout: a list of pages on the left, the page on the right.
        let stack = gtk::Stack::new();
        stack.set_hexpand(true);
        let pages = gtk::ListBox::new();
        pages.style_context().add_class("sidebar");
        pages.set_size_request(170, -1);
        let add_page = |name: &str, title: &str, icon: &str, content: &gtk::Box| {
            let sw = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
            sw.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
            sw.add(content);
            stack.add_named(&sw, name);
            let hb = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            hb.set_border_width(8);
            hb.add(&gtk::Image::from_icon_name(Some(icon), gtk::IconSize::Menu));
            hb.add(&gtk::Label::new(Some(title)));
            let row = gtk::ListBoxRow::new();
            row.add(&hb);
            row.set_widget_name(name);
            pages.add(&row);
        };
        let f = Form { dialog: d.clone(), labels: gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal) };
        add_page("views", &tr("Views"), "view-grid-symbolic", &self.views_page(&f, &s));
        add_page("behavior", &tr("Behavior"), "view-more-symbolic", &self.behavior_page(&f, &s));
        add_page("display", &tr("Display"), "view-reveal-symbolic", &self.display_page(&f, &s));
        add_page("columns", &tr("List Columns"), "view-list-symbolic", &self.columns_page(&f));
        add_page("previews", &tr("Previews"), "image-x-generic-symbolic", &self.previews_page(&f, &s));
        add_page("sidepane", &tr("Side Pane"), "view-list-symbolic", &self.side_pane_page(&f, &s));
        add_page("media", &tr("Media"), "media-removable-symbolic", &self.media_page(&f, &s));
        let (cm, items_refresh, bg_refresh) = self.context_menu_page(&f);
        add_page("contextmenu", &tr("Context Menu"), "open-menu-symbolic", &cm);
        add_page("customactions", &tr("Custom Actions"), "system-run-symbolic", &self.custom_actions_page(&f));
        add_page("properties", &tr("Properties"), "document-properties-symbolic", &self.properties_page(&f));
        if let Some(app) = self.window.application() {
            add_page("keyboard", &tr("Keyboard"), "input-keyboard-symbolic", &self.keyboard_page(&f, &app));
        }

        {
            let stack = stack.clone();
            pages.connect_row_selected(move |_, row| {
                if let Some(r) = row {
                    // Custom actions may have changed since the lists were filled.
                    if r.widget_name() == "contextmenu" {
                        items_refresh();
                        bg_refresh();
                    }
                    stack.set_visible_child_name(&r.widget_name());
                }
            });
        }
        pages.select_row(pages.row_at_index(0).as_ref());
        let hb = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        hb.add(&pages);
        hb.add(&gtk::Separator::new(gtk::Orientation::Vertical));
        hb.add(&stack);
        let area = d.content_area();
        area.set_border_width(0);
        area.pack_start(&hb, true, true, 0);
        d.connect_response(|d, _| d.close());
        d.show_all();
    }

    /// The default view and sorting, folders remembered, zoom levels, the list view.
    fn views_page(self: &Rc<Self>, f: &Form, s: &config::Settings) -> gtk::Box {
        // Views
        let v = Form::page();
        f.head(&v, &tr("Default View"));
        let view = gtk::ComboBoxText::new();
        view.append(Some("icon"), &tr("Icon View"));
        view.append(Some("list"), &tr("List View"));
        view.append(Some("compact"), &tr("Compact View"));
        view.append(Some("columns"), &tr("Columns View"));
        view.set_active_id(Some(&s.default_view));
        view.connect_changed(|c| {
            if let Some(id) = c.active_id() {
                config::update(|s| s.default_view = id.to_string());
            }
        });
        f.option(&v, &tr("View new folders using:"), view.upcast_ref());
        let arrange = gtk::ComboBoxText::new();
        arrange.append(Some("name"), &tr("By Name"));
        for d in crate::details::DETAILS {
            if let Some(label) = d.menu {
                arrange.append(Some(d.id), &tr(label).replace('_', ""));
            }
        }
        arrange.set_active_id(Some(&s.sort_by));
        arrange.connect_changed(|c| {
            if let Some(id) = c.active_id() {
                config::update(|s| s.sort_by = id.to_string());
                reapply_view_settings();
            }
        });
        f.option(&v, &tr("Arrange items:"), arrange.upcast_ref());
        let rev = setting_check(&tr("_Reverse sort"), None, s.sort_desc, |s, v| s.sort_desc = v, |_| reapply_view_settings());
        f.indent(&v, rev.upcast_ref());
        let ff = f.check(&tr("Sort _folders before files"));
        ff.set_action_name(Some("win.folders-first"));
        f.indent(&v, ff.upcast_ref());
        f.head(&v, &tr("Individual Folders"));
        let per = setting_check(
            &tr("Remember _view and sorting for each folder"),
            Some(&tr("Changing the view or sorting then only affects the folder you're in, and it's restored the next time you open that folder. Other folders use the defaults above.")),
            s.per_folder,
            |s, v| s.per_folder = v,
            |_| reapply_view_settings(),
        );
        f.indent(&v, per.upcast_ref());
        let remembered = |n: usize| match n {
            0 => tr("No folders remembered"),
            n => fill(&trn("{n} folder remembered", "{n} folders remembered", n as u64), &[("n", &n)]),
        };
        let count = gtk::Label::new(Some(&remembered(folders::count())));
        count.style_context().add_class("dim-label");
        let forget = gtk::Button::with_mnemonic(&tr("_Forget All"));
        forget.set_tooltip_text(Some(&tr("Forget the view and sorting of every folder, so they all use the defaults again")));
        forget.set_sensitive(folders::count() > 0);
        {
            let count = count.clone();
            forget.connect_clicked(move |b| {
                folders::forget_all();
                count.set_text(&remembered(0));
                b.set_sensitive(false);
                reapply_view_settings();
            });
        }
        {
            // Folders get remembered while the dialog is open.
            let (count, forget) = (count.clone(), forget.clone());
            f.dialog.connect_focus_in_event(move |_, _| {
                let n = folders::count();
                count.set_text(&remembered(n));
                forget.set_sensitive(n > 0);
                glib::Propagation::Proceed
            });
        }
        let hb = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hb.add(&count);
        hb.add(&forget);
        f.indent(&v, hb.upcast_ref());
        f.head(&v, &tr("Zoom"));
        v.add(&self.zoom_table(f));
        f.head(&v, &tr("List View"));
        let list_icons = setting_check(&tr("Show _icons"), Some(&tr("Icons beside the names in the list view")), s.list_icons, |s, v| s.list_icons = v, |_| for_all_panes(|p| p.apply_zoom()));
        f.indent(&v, list_icons.upcast_ref());
        v
    }

    /// The zoom table: per view a row with the zoom level and an editable
    /// list of the levels Zoom In and Zoom Out step through.
    fn zoom_table(self: &Rc<Self>, f: &Form) -> gtk::Grid {
        // The zoom table: per view a row with the zoom level dropdown and an
        // editable list of levels. `get`/`set` pick that view's
        // (levels, size) out of the settings.
        let zoom_grid = gtk::Grid::new();
        zoom_grid.set_row_spacing(6);
        zoom_grid.set_column_spacing(12);
        zoom_grid.set_margin_start(40);
        for (col, text) in [(1, tr("Zoom level")), (2, tr("Zoom levels to step through"))] {
            let l = gtk::Label::new(Some(&text));
            l.set_xalign(0.0);
            l.style_context().add_class("dim-label");
            zoom_grid.attach(&l, col, 0, 1, 1);
        }
        let zoom_row = |row: i32, title: &str, defaults: &'static [i32], get: fn(&config::Settings) -> (Vec<i32>, i32), set: fn(&mut config::Settings, Option<Vec<i32>>, i32)| {
            let l = gtk::Label::new(Some(title));
            l.set_xalign(0.0);
            zoom_grid.attach(&l, 0, row, 1, 1);
            let combo = gtk::ComboBoxText::new();
            let refill = {
                let combo = combo.clone();
                move || {
                    let (levels, size) = get(&config::get());
                    combo.remove_all();
                    for l in &levels {
                        combo.append(Some(&l.to_string()), &format!("{l} px"));
                    }
                    combo.set_active_id(Some(&size.to_string()));
                }
            };
            refill();
            // Zooming in a window changes the level while the dialog is open.
            {
                let refill = refill.clone();
                f.dialog.connect_focus_in_event(move |_, _| {
                    refill();
                    glib::Propagation::Proceed
                });
            }
            combo.connect_changed(move |c| {
                if let Some(px) = c.active_id().and_then(|i| i.parse().ok()) {
                    if get(&config::get()).1 != px {
                        config::update(|s| set(s, None, px));
                        for_all_panes(|p| p.apply_zoom());
                    }
                }
            });
            zoom_grid.attach(&combo, 1, row, 1, 1);
            let entry = gtk::Entry::new();
            entry.set_text(&config::format_levels(&get(&config::get()).0));
            entry.set_width_chars(26);
            entry.set_tooltip_text(Some(&tr("Sizes in pixels (8–512) that Zoom In and Zoom Out step through, separated by commas")));
            let reset = gtk::Button::from_icon_name(Some("edit-undo-symbolic"), gtk::IconSize::Button);
            reset.set_tooltip_text(Some(&tr("Restore the default zoom levels")));
            reset.set_sensitive(get(&config::get()).0 != defaults);
            let apply = {
                let reset = reset.clone();
                move |e: &gtk::Entry| {
                    // An emptied field goes back to the saved levels.
                    if e.text().trim().is_empty() {
                        e.set_text(&config::format_levels(&get(&config::get()).0));
                    }
                    match config::parse_levels(&e.text()) {
                        Some(levels) => {
                            e.style_context().remove_class("error");
                            reset.set_sensitive(levels != defaults);
                            e.set_text(&config::format_levels(&levels));
                            if levels != get(&config::get()).0 {
                                let size = config::snap(&levels, get(&config::get()).1);
                                config::update(|s| set(s, Some(levels), size));
                                refill();
                                for_all_panes(|p| p.apply_zoom());
                            }
                        }
                        None => e.style_context().add_class("error"),
                    }
                }
            };
            let apply = Rc::new(apply);
            {
                let apply = apply.clone();
                let entry = entry.clone();
                reset.connect_clicked(move |_| {
                    entry.set_text(&config::format_levels(defaults));
                    apply(&entry);
                });
            }
            {
                let apply = apply.clone();
                entry.connect_activate(move |e| apply(e));
            }
            entry.connect_focus_out_event(move |e, _| {
                apply(e);
                glib::Propagation::Proceed
            });
            let hb = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            hb.add(&entry);
            hb.add(&reset);
            zoom_grid.attach(&hb, 2, row, 1, 1);
        };
        zoom_row(1, &tr("Icon view"), &config::ICON_SIZES, |s| (s.icon_levels.clone(), s.icon_size), |s, levels, size| {
            if let Some(l) = levels {
                s.icon_levels = l;
            }
            s.icon_size = size;
        });
        zoom_row(2, &tr("List view"), &config::LIST_SIZES, |s| (s.list_levels.clone(), s.list_size), |s, levels, size| {
            if let Some(l) = levels {
                s.list_levels = l;
            }
            s.list_size = size;
        });
        zoom_row(3, &tr("Compact view"), &config::COMPACT_SIZES, |s| (s.compact_levels.clone(), s.compact_size), |s, levels, size| {
            if let Some(l) = levels {
                s.compact_levels = l;
            }
            s.compact_size = size;
        });
        zoom_grid
    }

    /// Opening, tabs, split view, the trash, other programs, scrolling.
    fn behavior_page(self: &Rc<Self>, f: &Form, s: &config::Settings) -> gtk::Box {
        // Behavior
        let b = Form::page();
        f.head(&b, &tr("Opening"));
        let single = setting_check(&tr("_Single click to open items"), None, s.single_click, |s, v| s.single_click = v, |_| for_all_panes(|p| p.apply_click_policy()));
        f.indent(&b, single.upcast_ref());
        f.head(&b, &tr("Tabs"));
        let tabs = gtk::ComboBoxText::new();
        for (id, name) in [("in-order", tr("After the current tab, in order")), ("after", tr("Right after the current tab")), ("end", tr("At the end"))] {
            tabs.append(Some(id), &name);
        }
        tabs.set_active_id(Some(&s.new_tabs));
        tabs.set_tooltip_text(Some(&tr(
            "After the current tab, in order: a folder opened in a new tab goes right after the current tab, and the next \
             ones after it, in the order opened, until you choose another tab; Ctrl+T adds a tab at the end.\n\
             Right after the current tab: each new tab goes right next to it, the newest first.",
        )));
        tabs.connect_changed(|c| {
            if let Some(id) = c.active_id() {
                config::update(|s| s.new_tabs = id.to_string());
            }
        });
        f.option(&b, &tr("New tabs go:"), tabs.upcast_ref());
        // Where New Tab opens (not a folder opened in a new tab: that one).
        let tab_opens = opens_choice(
            [tr("The same folder as the current tab"), tr("Your home folder"), tr("The folder of the last closed tab"), tr("This folder:")],
            &s.tab_opens,
            &s.tab_folder,
            &tr("Select the folder for new tabs"),
            |s, v| s.tab_opens = v,
            |s, v| s.tab_folder = v,
        );
        tab_opens.set_tooltip_text(Some(&tr("For New Tab (Ctrl+T); a folder opened in a new tab is shown there as before")));
        f.option(&b, &tr("New tabs open in:"), tab_opens.upcast_ref());
        f.head(&b, &tr("Split View"));
        let ffm = f.check(&tr("_Focus the pane under the mouse pointer"));
        ffm.set_tooltip_text(Some(&tr("With two panes open (F3), moving the pointer over a pane makes it the active one, without clicking (Shift+F3 switches this)")));
        ffm.set_action_name(Some("win.focus-follows-mouse"));
        f.indent(&b, ffm.upcast_ref());
        let delay = gtk::SpinButton::with_range(0.0, 2000.0, 50.0);
        delay.set_value(s.focus_follows_mouse_delay as f64);
        delay.set_tooltip_text(Some(&tr("How long the pointer has to stay over a pane before it becomes the active one, so passing over it on the way elsewhere (the side pane, say) doesn't change the active pane")));
        delay.connect_value_changed(|d| {
            let v = d.value_as_int();
            config::update(|s| s.focus_follows_mouse_delay = v);
        });
        ffm.bind_property("active", &delay, "sensitive").sync_create().build();
        let after = f.inline(&b, &tr("After the pointer rests on it for"), &[delay.upcast_ref(), gtk::Label::new(Some("ms")).upcast_ref()]);
        after.set_margin_start(after.margin_start() + 24);
        // Which pane stays when split view is turned off.
        let keep = gtk::ComboBoxText::new();
        keep.append(Some("active"), &tr("The pane in use"));
        keep.append(Some("other"), &tr("The other pane"));
        if !keep.set_active_id(Some(&s.split_keep)) {
            keep.set_active_id(Some("active"));
        }
        keep.connect_changed(|c| {
            if let Some(id) = c.active_id() {
                config::update(|s| s.split_keep = id.to_string());
            }
        });
        f.option(&b, &tr("When closing split view, keep:"), keep.upcast_ref());
        // Where the extra pane opens.
        let pane_opens = opens_choice(
            [tr("The same folder as the pane in use"), tr("Your home folder"), tr("The folder it showed when last closed"), tr("This folder:")],
            &s.split_opens,
            &s.split_folder,
            &tr("Select the folder for the extra pane"),
            |s, v| s.split_opens = v,
            |s, v| s.split_folder = v,
        );
        f.option(&b, &tr("The extra pane opens in:"), pane_opens.upcast_ref());
        f.head(&b, &tr("File Operations"));
        let own = setting_check(
            &tr("Show file operations in their own _window"),
            Some(&tr("Instead of the button in the location bar and its popup: the File Operations window opens when an operation takes a while, and closes when the last one is done")),
            s.ops_window,
            |s, v| s.ops_window = v,
            |_| crate::window::ops_shown_as_set(),
        );
        f.indent(&b, own.upcast_ref());
        f.head(&b, &tr("Trash"));
        let confirm = setting_check(&tr("Ask before moving files to the T_rash"), None, s.confirm_trash, |s, v| s.confirm_trash = v, |_| {});
        f.indent(&b, confirm.upcast_ref());
        f.head(&b, &tr("Search"));
        let sub = setting_check(
            &tr("_Include subfolders"),
            Some(&tr("A new search (Ctrl+F) also looks in the folders inside the open one, and in theirs. Each search can change it above its results: “include subfolders”.")),
            s.search_subfolders,
            |s, v| s.search_subfolders = v,
            |_| {},
        );
        f.indent(&b, sub.upcast_ref());
        f.head(&b, &tr("Archives"));
        // The installed ones (and the one chosen, if it's gone since).
        let archivers = gtk::ComboBoxText::new();
        archivers.append(Some(""), &tr("Automatic"));
        let mut any = false;
        for cmd in util::ARCHIVERS {
            let installed = glib::find_program_in_path(cmd).is_some();
            any |= installed;
            let name = util::archiver_name(cmd);
            if installed {
                archivers.append(Some(cmd), name);
            } else if s.archiver == cmd {
                archivers.append(Some(cmd), &fill(&tr("{name} (not installed)"), &[("name", &name)]));
            }
        }
        if !archivers.set_active_id(Some(&s.archiver)) {
            archivers.set_active_id(Some(""));
        }
        archivers.connect_changed(|c| {
            if let Some(id) = c.active_id() {
                config::update(|s| s.archiver = id.to_string());
            }
        });
        f.option(&b, &tr("Archive manager:"), archivers.upcast_ref());
        let hint = if any {
            tr("For Extract Here and Compress. Automatic: the one set as the application for archives, else the first one installed.")
        } else {
            tr("For Extract Here and Compress, none is installed: Engrampa, File Roller, Xarchiver or Ark.")
        };
        f.indent(&b, hint_label(&hint).upcast_ref());
        f.head(&b, &tr("Other Programs"));
        let fm1 = f.check(&tr("Open folders when other programs _ask, also when Arkha isn't running"));
        fm1.set_tooltip_text(Some(&tr("“Show in folder” in web browsers, download lists and the like opens Arkha, with the file selected. While Arkha runs it does this anyway, unless another file manager that does it was there first; this lets programs start Arkha for it too.")));
        fm1.set_active(crate::fm1::activation_on());
        {
            let d = f.dialog.clone();
            fm1.connect_toggled(move |c| {
                if let Err(e) = crate::fm1::set_activation(c.is_active()) {
                    util::error_dialog(Some(d.upcast_ref()), &tr("Could not change it"), &e.to_string());
                    c.set_active(crate::fm1::activation_on());
                }
            });
        }
        f.indent(&b, fm1.upcast_ref());
        f.head(&b, &tr("Scrolling"));
        let auto = setting_check(
            &tr("_Middle-click on empty space (or an item in the list view) to autoscroll"),
            Some(&tr("A marker appears where you clicked and the view scrolls towards the pointer, faster the further away it is. Click, press a key or use the wheel to stop (after dragging with the button held, letting go stops it). In the list view this works on files and folders too, if the list can scroll; otherwise, or with Shift, a middle-click opens them there (a folder in a new tab).")),
            s.autoscroll,
            |s, v| s.autoscroll = v,
            |_| {},
        );
        f.indent(&b, auto.upcast_ref());
        b
    }

    /// Files, icon captions, the window's parts, the location bar.
    fn display_page(self: &Rc<Self>, f: &Form, s: &config::Settings) -> gtk::Box {
        // Display
        let dp = Form::page();
        f.head(&dp, &tr("Language"));
        // The system's, English, or a translation; each by its own name.
        let lang = gtk::ComboBoxText::new();
        lang.append(Some(""), &tr("System default"));
        lang.append(Some("en"), "English");
        for (code, name) in crate::i18n::LANGUAGES {
            lang.append(Some(code), name);
        }
        if !lang.set_active_id(Some(&s.language)) {
            lang.set_active_id(Some(""));
        }
        let restart = gtk::Label::new(Some(&tr("Applies the next time Arkha starts.")));
        restart.set_xalign(0.0);
        restart.style_context().add_class("dim-label");
        restart.set_no_show_all(true);
        {
            let restart = restart.clone();
            lang.connect_changed(move |c| {
                let id = c.active_id().map(|i| i.to_string()).unwrap_or_default();
                config::update(|s| s.language = id);
                restart.show();
            });
        }
        f.option(&dp, &tr("Language:"), lang.upcast_ref());
        f.indent(&dp, restart.upcast_ref());
        f.head(&dp, &tr("Files"));
        let hid = f.check(&tr("Show _hidden and backup files"));
        hid.set_action_name(Some("win.show-hidden"));
        f.indent(&dp, hid.upcast_ref());
        let stripes = setting_check(&tr("_List view"), None, s.striped_rows, |s, v| s.striped_rows = v, |_| for_all_panes(|p| p.redraw()));
        let rename_stripes = setting_check(
            &tr("_Bulk rename list"),
            Some(&tr("Shade every second row of the file list in the Rename All… window (applies the next time it opens)")),
            s.rename_striped_rows,
            |s, v| s.rename_striped_rows = v,
            |_| {},
        );
        f.inline(&dp, &tr("Alternating row colors:"), &[stripes.upcast_ref(), rename_stripes.upcast_ref()]);
        let iec = setting_check(
            &tr("Show file sizes in _IEC units (KiB, MiB, GiB)"),
            Some(&tr("Sizes in powers of 1024 (1 KiB = 1024 bytes) instead of powers of 1000 (1 kB = 1000 bytes)")),
            s.iec_units,
            |s, v| s.iec_units = v,
            |v| {
                util::set_iec_units(v);
                // Sizes are formatted when rows are made.
                for_all_panes(|p| p.reload());
            },
        );
        f.indent(&dp, iec.upcast_ref());
        // Each choice shows how a date looks with it, as in Caja.
        let dates = gtk::ComboBoxText::new();
        let now = glib::DateTime::now_local().map(|d| d.to_unix()).unwrap_or(0);
        for (id, name) in [("informal", tr("Informal")), ("locale", tr("Your language’s")), ("iso", tr("ISO"))] {
            util::set_date_format(id);
            // TRANSLATORS: a date format's name and how a date looks with it.
            dates.append(Some(id), &fill(&tr("{format}: {example}"), &[("format", &name), ("example", &util::format_time(now))]));
        }
        util::set_date_format(&s.date_format);
        dates.set_active_id(Some(&s.date_format));
        dates.connect_changed(|c| {
            if let Some(id) = c.active_id() {
                config::update(|s| s.date_format = id.to_string());
                util::set_date_format(&id);
                // Dates are formatted when rows are made.
                for_all_panes(|p| p.reload());
            }
        });
        f.option(&dp, &tr("Dates:"), dates.upcast_ref());
        f.head(&dp, &tr("Icon Captions"));
        let about = gtk::Label::new(Some(&tr("What shows under the names in the icon view. More of it shows as you zoom in.")));
        about.set_xalign(0.0);
        about.set_line_wrap(true);
        about.style_context().add_class("dim-label");
        f.indent(&dp, about.upcast_ref());
        for (i, label) in [tr("First:"), tr("Second:"), tr("Third:")].iter().enumerate() {
            let combo = gtk::ComboBoxText::new();
            combo.append(Some("none"), &tr("None"));
            for d in crate::details::choosable() {
                combo.append(Some(d.id), &tr(d.title));
            }
            combo.set_active_id(Some(s.icon_captions.get(i).map_or("none", |c| c.as_str())));
            combo.connect_changed(move |c| {
                let Some(id) = c.active_id() else { return };
                config::update(|s| {
                    s.icon_captions.resize(3, "none".into());
                    s.icon_captions[i] = id.to_string();
                });
                for_all_panes(|p| {
                    p.apply_zoom();
                    p.reload();
                });
            });
            f.option(&dp, label, combo.upcast_ref());
        }
        f.head(&dp, &tr("Window"));
        let side = f.check(&tr("Show the side _pane"));
        side.set_action_name(Some("win.show-sidebar"));
        f.indent(&dp, side.upcast_ref());
        let tb = f.check(&tr("Show the _main toolbar"));
        tb.set_action_name(Some("win.show-toolbar"));
        f.indent(&dp, tb.upcast_ref());
        let lb = f.check(&tr("Show the l_ocation bar"));
        lb.set_tooltip_text(Some(&tr("Hidden, it still shows while you type a location (Ctrl+L) or search (Ctrl+F)")));
        lb.set_action_name(Some("win.show-locbar"));
        f.indent(&dp, lb.upcast_ref());
        let status = f.check(&tr("Show the status_bar"));
        status.set_action_name(Some("win.show-statusbar"));
        f.indent(&dp, status.upcast_ref());
        let status_hidden = setting_check(
            &tr("Say how many items are _hidden"),
            Some(&tr("The status bar adds how many items of the folder are hidden, as in “12 items (3 hidden)”")),
            s.status_hidden,
            |s, v| s.status_hidden = v,
            |_| {
                for w in all_windows() {
                    if let Some(p) = w.current_pane() {
                        w.status.set_text(&p.status_text());
                    }
                }
            },
        );
        // A sub-option of the status bar: only of use while it's shown.
        status.bind_property("active", &status_hidden, "sensitive").sync_create().build();
        f.indent(&dp, status_hidden.upcast_ref());
        status_hidden.set_margin_start(status_hidden.margin_start() + 24);
        let tabs = setting_check(
            &tr("Always show the _tab bar"),
            Some(&tr("Also show it when a window has only one tab, so that tab can be dragged to another window or out into a new one")),
            s.always_show_tabs,
            |s, v| s.always_show_tabs = v,
            |_| all_windows().iter().for_each(|w| w.update_tabs_visibility()),
        );
        f.indent(&dp, tabs.upcast_ref());
        let text = setting_check(
            &tr("Show te_xt on toolbar buttons"),
            Some(&tr("The Back and Forward buttons show their name next to the icon")),
            s.toolbar_text,
            |s, v| s.toolbar_text = v,
            |v| {
                for w in all_windows() {
                    for l in &w.toolbar_labels {
                        l.set_visible(v);
                    }
                }
            },
        );
        f.indent(&dp, text.upcast_ref());
        f.head(&dp, &tr("Location Bar"));
        let as_text = setting_check(
            &tr("Always show the _location as text"),
            Some(&tr("Show the folder's path as editable text instead of buttons for each folder")),
            s.path_as_text,
            |s, v| s.path_as_text = v,
            |_| all_windows().iter().for_each(|w| w.apply_path_mode()),
        );
        f.indent(&dp, as_text.upcast_ref());
        let bar_button = |label: &str, tip: &str, on: bool, set: fn(&mut config::Settings, bool), widget: fn(&AppWindow) -> gtk::Widget| {
            setting_check(label, Some(tip), on, set, move |v| all_windows().iter().for_each(|w| widget(w).set_visible(v)))
        };
        let zoom_b = bar_button(&tr("_Zoom"), &tr("Zoom in, zoom out and the zoom level"), s.show_zoom_buttons, |s, v| s.show_zoom_buttons = v, |w| w.zoom_box.clone().upcast());
        let view_b = bar_button(&tr("Vi_ew"), &tr("Icon, list and compact view"), s.show_view_buttons, |s, v| s.show_view_buttons = v, |w| w.view_box.clone().upcast());
        let split_b = bar_button(&tr("_Split view"), &tr("Show or hide the extra pane (F3)"), s.show_split_button, |s, v| s.show_split_button = v, |w| w.split_box.clone().upcast());
        // Shown only while there are two panes to swap.
        let swap_b = setting_check(&tr("S_wap panes"), Some(&tr("Swap the two sides of split view (Ctrl+U); shown in split view")), s.show_swap_button, |s, v| s.show_swap_button = v, |v| {
            all_windows().iter().for_each(|w| w.swap_btn.set_visible(v && w.split_on()))
        });
        let info_b = bar_button(&tr("_Info panel"), &tr("Show or hide the info panel in the folder you're in (F11)"), s.show_info_button, |s, v| s.show_info_button = v, |w| w.info_btn.clone().upcast());
        f.inline(&dp, &tr("Buttons:"), &[zoom_b.upcast_ref(), view_b.upcast_ref(), split_b.upcast_ref(), swap_b.upcast_ref(), info_b.upcast_ref()]);
        dp
    }

    /// The list view's columns and their order, as in Caja.
    fn columns_page(self: &Rc<Self>, f: &Form) -> gtk::Box {
        // List Columns, as in Caja.
        let lc = Form::page();
        f.head(&lc, &tr("List Columns"));
        let about = gtk::Label::new(Some(&tr("Choose the order of information to appear in the list view. Columns can also be dragged to another place, and a right click on the column headers shows or hides them.")));
        about.set_xalign(0.0);
        about.set_line_wrap(true);
        about.set_max_width_chars(60);
        about.style_context().add_class("dim-label");
        f.indent(&lc, about.upcast_ref());
        f.indent(&lc, column_chooser().upcast_ref());
        lc
    }

    /// Text in icons, thumbnails, item counts, Media Info, the info panel.
    fn previews_page(self: &Rc<Self>, f: &Form, s: &config::Settings) -> gtk::Box {
        // Previews, as Caja's: text in icons, thumbnails, item counts (each
        // always, for local files only, or never), the Media Info tab.
        let pv = Form::page();
        let tradeoff = |b: &gtk::Box, label: &str, tip: &str, current: &str, set: fn(&mut config::Settings, String)| {
            let c = gtk::ComboBoxText::new();
            for (id, name) in [("always", tr("Always")), ("local", tr("Local files only")), ("never", tr("Never"))] {
                c.append(Some(id), &name);
            }
            c.set_active_id(Some(current));
            c.set_tooltip_text(Some(tip));
            c.connect_changed(move |c| {
                if let Some(id) = c.active_id() {
                    config::update(|s| set(s, id.to_string()));
                    for_all_panes(|p| p.reload());
                }
            });
            f.option(b, label, c.upcast_ref());
        };
        let local_tip = tr("Local files: those on this computer’s drives, network shares the system mounts included; not places opened through GVfs (smb://, sftp://, phones), which can be slow");
        // TRANSLATORS: a setting's explanation, then what “local files” are.
        let with_local = |text: String| fill(&tr("{text} {local_files}"), &[("text", &text), ("local_files", &local_tip)]);
        f.head(&pv, &tr("Text Files"));
        tradeoff(&pv, &tr("Show text in icons:"), &with_local(tr("At bigger icon sizes a text file’s icon shows its first lines.")), &s.text_in_icons, |s, v| s.text_in_icons = v);
        f.head(&pv, &tr("Other Previewable Files"));
        tradeoff(&pv, &tr("Show thumbnails:"), &with_local(tr("Images get thumbnails built in; other file types through the custom thumbnailers below, or the ones installed on the system.")), &s.show_thumbnails, |s, v| s.show_thumbnails = v);
        let limit = gtk::ComboBoxText::new();
        for (bytes, name) in [(100u64 << 10, "100 KB"), (500 << 10, "500 KB"), (1 << 20, "1 MB"), (3 << 20, "3 MB"), (5 << 20, "5 MB"), (10 << 20, "10 MB"), (100 << 20, "100 MB"), (1 << 30, "1 GB"), (2 << 30, "2 GB"), (4 << 30, "4 GB")] {
            limit.append(Some(&bytes.to_string()), name);
        }
        if !limit.set_active_id(Some(&s.thumbnail_limit.to_string())) {
            // A value set by hand in arkha.conf.
            limit.append(Some(&s.thumbnail_limit.to_string()), &util::format_size(s.thumbnail_limit));
            limit.set_active_id(Some(&s.thumbnail_limit.to_string()));
        }
        limit.set_tooltip_text(Some(&tr("Bigger images get no thumbnail: decoding them takes long. Other files' thumbnailers decide for themselves.")));
        limit.connect_changed(|c| {
            if let Some(bytes) = c.active_id().and_then(|id| id.parse::<u64>().ok()) {
                config::update(|s| s.thumbnail_limit = bytes);
                for_all_panes(|p| p.reload());
            }
        });
        f.option(&pv, &tr("Only for files smaller than:"), limit.upcast_ref());
        let thumbs = thumbnailers_editor(f.dialog.upcast_ref());
        thumbs.set_margin_start(40);
        pv.add(&thumbs);
        f.head(&pv, &tr("Folders"));
        tradeoff(&pv, &tr("Count number of items:"), &with_local(tr("A folder’s size is how many items it has.")), &s.count_items, |s, v| s.count_items = v);
        f.head(&pv, &tr("Media Info"));
        let mi = setting_check(
            &tr("Show a _Media Info tab in file properties"),
            Some(&tr("Shows what the mediainfo program reports about a file: codecs, resolution, duration, bit rate and more")),
            s.media_info,
            |s, v| s.media_info = v,
            |_| {},
        );
        f.indent(&pv, mi.upcast_ref());
        let kinds = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        kinds.add(&gtk::Label::new(Some(&tr("For:"))));
        let kind = |label: &str, on: bool, set: fn(&mut config::Settings, bool)| kinds.add(&setting_check(label, None, on, set, |_| {}));
        kind(&tr("_Videos"), s.media_info_video, |s, v| s.media_info_video = v);
        kind(&tr("_Audio"), s.media_info_audio, |s, v| s.media_info_audio = v);
        kind(&tr("_Images"), s.media_info_image, |s, v| s.media_info_image = v);
        kind(&tr("_Other files"), s.media_info_other, |s, v| s.media_info_other = v);
        kinds.set_sensitive(s.media_info);
        {
            let kinds = kinds.clone();
            mi.connect_toggled(move |c| kinds.set_sensitive(c.is_active()));
        }
        f.indent(&pv, kinds.upcast_ref());
        // mediainfo is optional: without it the settings stay visible but
        // greyed out, so it's clear what installing it would add.
        if !props::mediainfo_available() {
            mi.set_sensitive(false);
            kinds.set_sensitive(false);
            let note = hint_label(&tr("Install mediainfo to use this: the tab shows codecs, resolution, duration, bit rate and more."));
            f.indent(&pv, note.upcast_ref());
        }
        f.head(&pv, &tr("Info Panel"));
        let note = hint_label(&tr("Beside the files (View → Info Panel, F11, or its toolbar button), in the folders where you turn it on. It shows the selected item: a folder's cover picture, an image, or a file's thumbnail. Only local folders are searched for covers."));
        f.indent(&pv, note.upcast_ref());
        let names = gtk::Entry::new();
        names.set_text(&s.cover_names);
        names.set_width_chars(36);
        names.set_tooltip_text(Some(&tr("File names to use as a folder's cover, separated by semicolons, in order of preference. Upper and lower case don't matter.")));
        let apply_names = |e: &gtk::Entry| {
            let mut v = e.text().trim().to_string();
            if v.is_empty() {
                v = config::Settings::default().cover_names;
                e.set_text(&v);
            }
            if v != config::get().cover_names {
                config::update(|s| s.cover_names = v);
                for_all_panes(|p| p.refresh_info());
            }
        };
        names.connect_activate(apply_names);
        names.connect_focus_out_event(move |e, _| {
            apply_names(e);
            glib::Propagation::Proceed
        });
        f.option(&pv, &tr("Cover file names:"), names.upcast_ref());
        let choice = |items: &[(&str, &str)], current: &str, set: fn(&mut config::Settings, String)| {
            let c = gtk::ComboBoxText::new();
            for (id, label) in items {
                c.append(Some(id), &tr(label));
            }
            c.set_active_id(Some(current));
            c.connect_changed(move |c| {
                if let Some(id) = c.active_id() {
                    config::update(|s| set(s, id.to_string()));
                    for_all_panes(|p| p.refresh_info());
                }
            });
            c
        };
        let other = choice(&[("summary", n_("Show their number and total size")), ("last", n_("Keep showing the last item")), ("current", n_("Show the open folder")), ("hide", n_("Hide the panel"))], &s.info_several, |s, v| s.info_several = v);
        f.option(&pv, &tr("With several items selected:"), other.upcast_ref());
        let missing = choice(&[("icon", n_("Show its icon and the details")), ("none", n_("Show nothing"))], &s.info_no_cover, |s, v| s.info_no_cover = v);
        f.option(&pv, &tr("Folders without a cover:"), missing.upcast_ref());
        let details = setting_check(&tr("Show the _details (type, size, date, …)"), None, s.info_details, |s, v| s.info_details = v, |_| for_all_panes(|p| p.refresh_info()));
        f.indent(&pv, details.upcast_ref());
        pv
    }

    /// The side pane's places, their order and its row height.
    fn side_pane_page(self: &Rc<Self>, f: &Form, s: &config::Settings) -> gtk::Box {
        // Side Pane
        let sp = Form::page();
        f.head(&sp, &tr("Computer Entries"));
        let hint = hint_label(&tr("Unchecked entries are hidden. Drag an entry by its handle, or select it and use the arrows, to move it; they can also be dragged in the side pane itself."));
        f.indent(&sp, hint.upcast_ref());
        let sidebar = self.sidebar.clone();
        let (places, places_refresh) = places_editor(move || sidebar.places());
        f.indent(&sp, places.upcast_ref());
        // Entries dragged in the side pane while the dialog is open.
        f.dialog.connect_focus_in_event(move |_, _| {
            places_refresh();
            glib::Propagation::Proceed
        });
        f.head(&sp, &tr("Appearance"));
        let custom = f.check(&tr("Custom _row height:"));
        let height = gtk::SpinButton::with_range(16.0, 80.0, 1.0);
        height.set_value(if s.sidebar_row_height > 0 { s.sidebar_row_height } else { 36 } as f64);
        custom.set_active(s.sidebar_row_height > 0);
        height.set_sensitive(custom.is_active());
        let set_height = {
            let (custom, height) = (custom.clone(), height.clone());
            move || {
                let h = if custom.is_active() { height.value_as_int() } else { 0 };
                height.set_sensitive(custom.is_active());
                config::update(|s| s.sidebar_row_height = h);
                apply_sidebar_css();
            }
        };
        {
            let f = set_height.clone();
            custom.connect_toggled(move |_| f());
        }
        height.connect_value_changed(move |_| set_height());
        let hb = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        hb.add(&custom);
        hb.add(&height);
        hb.add(&gtk::Label::new(Some("pixels")));
        f.indent(&sp, hb.upcast_ref());
        sp
    }

    /// Mounting, then what happens with what's mounted.
    fn media_page(self: &Rc<Self>, f: &Form, s: &config::Settings) -> gtk::Box {
        // Media: mounting first, then what happens with what's mounted.
        let media = crate::autorun::preferences_page();
        let removable = Form::page();
        f.head(&removable, &tr("Removable Media"));
        let automount = f.check(&tr("Mount drives and media when they're _inserted"));
        automount.set_tooltip_text(Some(&match crate::sidebar::automount_desktop() {
            Some(desktop) => fill(&tr("Flash drives, memory cards, discs… are mounted right away. This is {desktop}’s own setting: changing it here changes it there too. Turned off, they’re mounted when you click them in the side pane."), &[("desktop", &desktop)]),
            None => tr("Flash drives, memory cards, discs… inserted while Arkha runs are mounted right away. Turned off, they’re mounted when you click them in the side pane."),
        }));
        automount.set_active(crate::sidebar::automount_on());
        automount.connect_toggled(|c| crate::sidebar::set_automount(c.is_active()));
        // It may have been changed in the desktop's settings meanwhile.
        f.dialog.connect_focus_in_event(glib::clone!(@weak automount => @default-return glib::Propagation::Proceed, move |_, _| {
            let on = crate::sidebar::automount_on();
            if automount.is_active() != on {
                automount.set_active(on);
            }
            glib::Propagation::Proceed
        }));
        f.indent(&removable, automount.upcast_ref());
        let unmount_only = setting_check(
            &tr("The eject button only _unmounts"),
            Some(&tr("Clicking the button next to a drive in the side pane unmounts it, but doesn't eject it or power it off (the drive stays in the list, ready to be mounted again)")),
            s.unmount_only,
            |s, v| s.unmount_only = v,
            |_| crate::sidebar::refresh_all(),
        );
        f.indent(&removable, unmount_only.upcast_ref());
        for (i, w) in removable.children().iter().enumerate() {
            removable.remove(w);
            media.add(w);
            media.reorder_child(w, i as i32);
        }
        media
    }

    /// The context menus' entries: their order, which are shown. Also what
    /// fills the two lists again (custom actions may have changed).
    fn context_menu_page(self: &Rc<Self>, f: &Form) -> (gtk::Box, Refill, Refill) {
        // Context Menu: order and visibility of the entries, per menu
        let cm = Form::page();
        f.head(&cm, &tr("Context Menu Entries"));
        let hint = hint_label(&tr("Drag an entry by its handle, or select it and use the arrows, to move it. Unchecked entries are hidden. Entries that don't apply are left out of the menu (Restore outside the Trash, say)."));
        f.indent(&cm, hint.upcast_ref());
        let menus = gtk::Stack::new();
        let (items_editor, items_refresh) = menu_order_editor(false);
        let (bg_editor, bg_refresh) = menu_order_editor(true);
        menus.add_titled(&items_editor, "items", &tr("Files and Folders"));
        menus.add_titled(&bg_editor, "background", &tr("A Folder’s Empty Space"));
        let switcher = gtk::StackSwitcher::new();
        switcher.set_stack(Some(&menus));
        switcher.set_halign(gtk::Align::Start);
        f.indent(&cm, switcher.upcast_ref());
        f.indent(&cm, menus.upcast_ref());
        (cm, items_refresh, bg_refresh)
    }

    /// Custom actions in the context menus.
    fn custom_actions_page(self: &Rc<Self>, f: &Form) -> gtk::Box {
        // Custom Actions (in the context menus)
        let ca = Form::page();
        f.head(&ca, &tr("Custom Actions in the Context Menu"));
        let editor = custom_actions_editor(f.dialog.upcast_ref());
        editor.set_margin_start(40);
        ca.add(&editor);
        ca
    }

    /// The Properties window: which tabs it shows and in what order, and
    /// the checksums its Checksums tab offers.
    fn properties_page(self: &Rc<Self>, f: &Form) -> gtk::Box {
        let page = Form::page();
        f.head(&page, &tr("Tabs"));
        let hint = hint_label(&tr("Unchecked tabs aren't shown in the Properties window. Drag a tab by its handle, or select it and use the arrows, to move it; dragging the tabs in the Properties window moves them here too."));
        f.indent(&page, hint.upcast_ref());
        let tabs = props_tabs_editor();
        tabs.set_margin_start(40);
        page.pack_start(&tabs, false, false, 0);
        f.head(&page, &tr("Checksums"));
        let hint = hint_label(&tr("Unchecked ones aren't offered. Drag one by its handle, or select it and use the arrows, to move it."));
        f.indent(&page, hint.upcast_ref());
        let editor = checksums_editor();
        editor.set_margin_start(40);
        page.pack_start(&editor, true, true, 0);
        page
    }

    /// Keyboard shortcuts.
    fn keyboard_page(self: &Rc<Self>, f: &Form, app: &gtk::Application) -> gtk::Box {
        let kb = Form::page();
        f.head(&kb, &tr("Keyboard Shortcuts"));
        let editor = keybindings_editor(f.dialog.upcast_ref(), app);
        editor.set_margin_start(40);
        kb.pack_start(&editor, true, true, 0);
        kb
    }
}

/// Fills a list in Preferences again.
type Refill = Rc<dyn Fn()>;

/// What Preferences' pages are laid out with.
struct Form {
    dialog: gtk::Dialog,
    /// Lines up the controls of "Label:  [control]" rows.
    labels: gtk::SizeGroup,
}

impl Form {
    fn page() -> gtk::Box {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
        b.set_border_width(14);
        b
    }

    /// A bold heading, with room above it unless it's the page's first.
    fn head(&self, b: &gtk::Box, t: &str) {
        let l = gtk::Label::new(None);
        l.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(t)));
        l.set_xalign(0.0);
        if !b.children().is_empty() {
            l.set_margin_top(8);
        }
        b.add(&l);
    }

    fn indent(&self, b: &gtk::Box, w: &gtk::Widget) {
        w.set_margin_start(40);
        b.add(w);
    }

    /// "Label:  [control]".
    fn option(&self, b: &gtk::Box, text: &str, w: &gtk::Widget) {
        let hb = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let l = gtk::Label::new(Some(text));
        l.set_xalign(0.0);
        self.labels.add_widget(&l);
        hb.add(&l);
        hb.add(w);
        self.indent(b, hb.upcast_ref());
    }

    fn check(&self, label: &str) -> gtk::CheckButton {
        gtk::CheckButton::with_mnemonic(label)
    }

    /// Related switches on one line: "Label:  [x] A  [x] B".
    fn inline(&self, b: &gtk::Box, text: &str, items: &[&gtk::Widget]) -> gtk::Box {
        let hb = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hb.add(&gtk::Label::new(Some(text)));
        for w in items {
            hb.add(*w);
        }
        self.indent(b, hb.upcast_ref());
        hb
    }
}

const MENU_ROW_TARGET: &str = "ARKHA_MENU_ROW";
const PLACE_ROW_TARGET: &str = "ARKHA_PLACE_ROW";
const CHECKSUM_ROW_TARGET: &str = "ARKHA_CHECKSUM_ROW";
const PROPS_TAB_ROW_TARGET: &str = "ARKHA_PROPS_TAB_ROW";

/// A check box for a setting: `set` stores it when it's toggled, then
/// `then` applies it.
fn setting_check(label: &str, tip: Option<&str>, on: bool, set: fn(&mut config::Settings, bool), then: impl Fn(bool) + 'static) -> gtk::CheckButton {
    let c = gtk::CheckButton::with_mnemonic(label);
    c.set_tooltip_text(tip);
    c.set_active(on);
    c.connect_toggled(move |c| {
        let v = c.is_active();
        config::update(|s| set(s, v));
        then(v);
    });
    c
}

/// A row of buttons under a list: `start` from the left, `end` at the right.
fn button_row(start: &[&gtk::Button], end: &[&gtk::Button]) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    for b in start {
        row.pack_start(*b, false, false, 0);
    }
    for b in end.iter().rev() {
        row.pack_end(*b, false, false, 0);
    }
    row
}

/// A dialog to add or change an item of a list, with Cancel and Save, and
/// the grid for its fields.
fn edit_dialog(parent: &gtk::Window, title: &str) -> (gtk::Dialog, gtk::Grid) {
    let d = gtk::Dialog::with_buttons(
        Some(title),
        Some(parent),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        &[(tr("_Cancel").as_str(), gtk::ResponseType::Cancel), (tr("_Save").as_str(), gtk::ResponseType::Ok)],
    );
    d.set_default_response(gtk::ResponseType::Ok);
    let grid = gtk::Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(12);
    grid.set_border_width(12);
    (d, grid)
}

/// A dimmed explanation under a setting, wrapped.
/// A choice of where something new opens (New Tab, split view's extra
/// pane): `labels` for "same", "home", "last" and "folder", the last one
/// with a folder button beside it, active only for it. `opens` and
/// `folder` are the settings now; `set_opens` and `set_folder` store them.
fn opens_choice(
    labels: [String; 4],
    opens: &str,
    folder: &str,
    title: &str,
    set_opens: fn(&mut config::Settings, String),
    set_folder: fn(&mut config::Settings, String),
) -> gtk::Box {
    let combo = gtk::ComboBoxText::new();
    for (id, label) in ["same", "home", "last", "folder"].into_iter().zip(labels) {
        combo.append(Some(id), &label);
    }
    if !combo.set_active_id(Some(opens)) {
        combo.set_active_id(Some("same"));
    }
    let chooser = gtk::FileChooserButton::new(title, gtk::FileChooserAction::SelectFolder);
    chooser.set_local_only(false);
    let start = if folder.is_empty() { util::home_file().uri().to_string() } else { folder.to_string() };
    let _ = chooser.set_current_folder_uri(&start);
    // "This folder:" with none stored yet (a settings file edited by hand):
    // the one the button shows, home, as it's what it says.
    if opens == "folder" && folder.is_empty() {
        config::update(|s| set_folder(s, start.clone()));
    }
    chooser.set_sensitive(opens == "folder");
    // Stored as the button shows it: a folder from its list (Home, File
    // System, …) only says selection-changed, one from its dialog file-set
    // too (current-folder-changed comes for neither); selection-changed
    // also comes once it's set up, so a folder shown is never one unstored.
    let store = move |c: &gtk::FileChooserButton| {
        if let Some(uri) = c.uri().or_else(|| c.current_folder_uri()) {
            config::update(|s| set_folder(s, uri.to_string()));
        }
    };
    chooser.connect_selection_changed(store);
    chooser.connect_file_set(store);
    {
        let chooser = chooser.clone();
        let picked = !folder.is_empty();
        combo.connect_changed(move |c| {
            let Some(id) = c.active_id() else { return };
            let id = id.to_string();
            chooser.set_sensitive(id == "folder");
            // The folder shown in the button, if none was picked yet.
            let shown = chooser.uri().or_else(|| chooser.current_folder_uri()).map(|u| u.to_string());
            config::update(|s| {
                if id == "folder" && !picked {
                    set_folder(s, shown.clone().unwrap_or_default());
                }
                set_opens(s, id.clone());
            });
        });
    }
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.add(&combo);
    row.add(&chooser);
    row
}

fn hint_label(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_line_wrap(true);
    l.set_max_width_chars(60);
    l.style_context().add_class("dim-label");
    l
}

/// A list the user puts in order, for the preferences: entries by key,
/// each row with a handle to drag it by; the selected one moves with the
/// arrows (see arrows). The context menus and the side pane's places use it.
struct OrderList {
    widget: gtk::ScrolledWindow,
    list: gtk::ListBox,
    order: Rc<RefCell<Vec<String>>>,
    /// Fills the list from `order`, selecting row `select`.
    rebuild: Rc<dyn Fn(Option<usize>)>,
    /// Moves an entry, tells the list's owner (`changed`) and rebuilds.
    move_to: Rc<dyn Fn(usize, usize)>,
}

impl OrderList {
    fn selected(&self) -> Option<usize> {
        self.list.selected_row().map(|r| r.index() as usize)
    }

    /// Buttons moving the selected entry up and down.
    fn arrows(self: &Rc<Self>) -> (gtk::Button, gtk::Button) {
        let up = gtk::Button::from_icon_name(Some("go-up-symbolic"), gtk::IconSize::Button);
        up.set_tooltip_text(Some(&tr("Move up")));
        let down = gtk::Button::from_icon_name(Some("go-down-symbolic"), gtk::IconSize::Button);
        down.set_tooltip_text(Some(&tr("Move down")));
        let ol = self.clone();
        up.connect_clicked(move |_| {
            if let Some(i) = ol.selected().filter(|i| *i > 0) {
                (ol.move_to)(i, i - 1);
            }
        });
        let ol = self.clone();
        down.connect_clicked(move |_| {
            if let Some(i) = ol.selected().filter(|i| i + 1 < ol.order.borrow().len()) {
                (ol.move_to)(i, i + 1);
            }
        });
        (up, down)
    }
}

/// An OrderList whose rows show `content(key)` beside their handle;
/// `changed` gets the new order when the user moves an entry. `target` is
/// the rows' own kind of drag: they can only be dropped in their list.
fn order_list(target: &'static str, content: impl Fn(&str) -> gtk::Widget + 'static, changed: impl Fn(&[String]) + 'static) -> Rc<OrderList> {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    let sw = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    sw.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    sw.set_shadow_type(gtk::ShadowType::In);
    sw.set_min_content_height(150);
    // Grows with the preferences window (the boxes around pass it on).
    sw.set_vexpand(true);
    sw.add(&list);
    let order: Rc<RefCell<Vec<String>>> = Rc::default();
    let handle_icon = if gtk::IconTheme::default().is_some_and(|t| t.has_icon("list-drag-handle-symbolic")) { "list-drag-handle-symbolic" } else { "open-menu-symbolic" };
    let rebuild: Rc<dyn Fn(Option<usize>)> = {
        let (list, order, sw) = (list.clone(), order.clone(), sw.clone());
        Rc::new(move |select| {
            for c in list.children() {
                list.remove(&c);
            }
            for (i, k) in order.borrow().iter().enumerate() {
                let hb = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                hb.set_margin_start(4);
                hb.set_margin_end(8);
                let handle = gtk::EventBox::new();
                handle.add(&gtk::Image::from_icon_name(Some(handle_icon), gtk::IconSize::Menu));
                handle.set_tooltip_text(Some(&tr("Drag to move")));
                let targets = [gtk::TargetEntry::new(target, gtk::TargetFlags::SAME_APP, 0)];
                handle.drag_source_set(gdk::ModifierType::BUTTON1_MASK, &targets, gdk::DragAction::MOVE);
                handle.connect_drag_begin(|h, ctx| {
                    if let Some(row) = h.ancestor(gtk::ListBoxRow::static_type()) {
                        crate::sidebar::drag_picture(&row, ctx);
                    }
                });
                // A custom target, so the row's index goes as raw bytes.
                handle.connect_drag_data_get(move |_, _, data, _, _| {
                    data.set(&data.target(), 8, i.to_string().as_bytes());
                });
                hb.add(&handle);
                let c = content(k);
                c.set_hexpand(true);
                hb.add(&c);
                let row = gtk::ListBoxRow::new();
                row.add(&hb);
                list.add(&row);
            }
            list.show_all();
            if let Some(r) = select.and_then(|i| list.row_at_index(i as i32)) {
                list.select_row(Some(&r));
                // Keep the moved entry in view (once the new rows have
                // their sizes).
                let adj = sw.vadjustment();
                glib::idle_add_local_once(move || {
                    // Not once the dialog is closed (see
                    // Pane::scroll_columns_to_end).
                    if !r.is_realized() {
                        return;
                    }
                    let a = r.allocation();
                    adj.clamp_page(a.y() as f64, (a.y() + a.height()) as f64);
                });
            }
        })
    };
    let move_to: Rc<dyn Fn(usize, usize)> = {
        let (order, rebuild) = (order.clone(), rebuild.clone());
        Rc::new(move |from, to| {
            let to = {
                let mut o = order.borrow_mut();
                if from >= o.len() || from == to {
                    return;
                }
                let k = o.remove(from);
                let to = to.min(o.len());
                o.insert(to, k);
                to
            };
            changed(&order.borrow());
            rebuild(Some(to));
        })
    };
    // Dropping a dragged row: it takes the place of the row it's dropped on.
    let targets = [gtk::TargetEntry::new(target, gtk::TargetFlags::SAME_APP, 0)];
    list.drag_dest_set(gtk::DestDefaults::empty(), &targets, gdk::DragAction::MOVE);
    list.connect_drag_motion(|l, ctx, _, y, time| {
        match l.row_at_y(y) {
            Some(r) => l.drag_highlight_row(&r),
            None => l.drag_unhighlight_row(),
        }
        ctx.drag_status(gdk::DragAction::MOVE, time);
        true
    });
    list.connect_drag_leave(|l, _, _| l.drag_unhighlight_row());
    // After a drag the list misses the next click (its click tracking
    // never saw the drag's button release), so rows are selected on press
    // here. Root coordinates: the press may come from a row's own widgets.
    list.connect_button_press_event(|l, e| {
        if e.button() == 1 {
            if let Some((_, oy)) = l.window().map(|w| w.origin()).map(|(_, x, y)| (x, y)) {
                if let Some(r) = l.row_at_y(e.root().1 as i32 - oy) {
                    l.select_row(Some(&r));
                }
            }
        }
        glib::Propagation::Proceed
    });
    list.connect_drag_drop(|l, ctx, _, _, time| {
        match l.drag_dest_find_target(ctx, None) {
            Some(t) => l.drag_get_data(ctx, &t, time),
            None => ctx.drag_finish(false, false, time),
        }
        true
    });
    {
        let (order, move_to) = (order.clone(), move_to.clone());
        list.connect_drag_data_received(move |l, ctx, _, y, data, _, time| {
            l.drag_unhighlight_row();
            let from = String::from_utf8(data.data()).ok().and_then(|t| t.parse::<usize>().ok());
            let last = order.borrow().len().saturating_sub(1);
            let to = l.row_at_y(y).map_or(last, |r| r.index() as usize);
            ctx.drag_finish(from.is_some(), false, time);
            if let Some(from) = from {
                move_to(from, to);
            }
        });
    }
    Rc::new(OrderList { widget: sw, list, order, rebuild, move_to })
}

/// A checkbox showing or hiding an entry: `key` in the hidden list that
/// `hidden` picks out of the settings. `then` runs after a change.
fn shown_check(label: &gtk::Widget, key: &str, hidden: fn(&mut config::Settings) -> &mut Vec<String>, then: fn()) -> gtk::CheckButton {
    let cb = gtk::CheckButton::new();
    cb.add(label);
    cb.set_active(!config::with(|s| hidden(&mut s.clone()).iter().any(|h| h == key)));
    let key = key.to_string();
    cb.connect_toggled(move |c| {
        let show = c.is_active();
        config::update(|s| {
            let list = hidden(s);
            list.retain(|x| *x != key);
            if !show {
                list.push(key.clone());
            }
        });
        then();
    });
    cb
}

/// The order of one context menu (`bg`: the one for a folder's empty space)
/// as a list: entries are moved by dragging their handle or with the arrow
/// buttons, shown or hidden with their checkbox, and separators can be
/// added and removed. Returns the editor and a function that reloads it
/// (custom actions may have been added or removed meanwhile).
fn menu_order_editor(bg: bool) -> (gtk::Box, Rc<dyn Fn()>) {
    let defaults = if bg { actions::BACKGROUND_ORDER } else { actions::ITEM_ORDER };
    let labels = if bg { actions::BACKGROUND_ENTRIES } else { actions::ITEM_ENTRIES };
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let save = move |o: &[String]| {
        let o = o.to_vec();
        config::update(|s| if bg { s.menu_order_bg = o } else { s.menu_order = o });
    };
    let row = move |k: &str| -> gtk::Widget {
        let hb = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        if k == actions::SEPARATOR {
            let l = gtk::Label::new(Some(&tr("Separator")));
            l.style_context().add_class("dim-label");
            hb.add(&l);
            let line = gtk::Separator::new(gtk::Orientation::Horizontal);
            line.set_hexpand(true);
            line.set_valign(gtk::Align::Center);
            hb.add(&line);
            return hb.upcast();
        }
        let action = actions::load().into_iter().find(|a| actions::custom_key(a) == k);
        let label = match &action {
            Some(a) => a.name.clone(),
            None => labels.iter().find(|(key, _)| *key == k).map_or(k.to_string(), |(_, l)| tr(l)),
        };
        hb.add(&shown_check(gtk::Label::new(Some(&label)).upcast_ref(), k, |s| &mut s.hidden_menu_items, || {}));
        if action.is_some() {
            let l = gtk::Label::new(Some(&tr("custom action")));
            l.style_context().add_class("dim-label");
            hb.add(&l);
        }
        hb.upcast()
    };
    let ol = order_list(MENU_ROW_TARGET, row, save);
    outer.add(&ol.widget);
    let refresh: Rc<dyn Fn()> = {
        let ol = ol.clone();
        Rc::new(move || {
            let custom = actions::load();
            let keys: Vec<String> = custom.iter().filter(|a| if bg { a.background } else { a.files || a.folders }).map(actions::custom_key).collect();
            let cfg = config::get();
            *ol.order.borrow_mut() = actions::menu_order(if bg { &cfg.menu_order_bg } else { &cfg.menu_order }, defaults, &keys);
            (ol.rebuild)(None);
        })
    };
    refresh();
    let (up, down) = ol.arrows();
    let add_sep = gtk::Button::with_mnemonic(&tr("Add _Separator"));
    add_sep.set_tooltip_text(Some(&tr("Add a separator below the selected entry")));
    let remove_sep = gtk::Button::with_mnemonic(&tr("Re_move Separator"));
    remove_sep.set_sensitive(false);
    let reset = gtk::Button::with_mnemonic(&tr("Reset _Order"));
    reset.set_tooltip_text(Some(&tr("Put every entry back in its original place (which entries are hidden stays as it is)")));
    {
        let ol = ol.clone();
        add_sep.connect_clicked(move |_| {
            let at = ol.selected().map_or(ol.order.borrow().len(), |i| i + 1);
            ol.order.borrow_mut().insert(at, actions::SEPARATOR.to_string());
            save(&ol.order.borrow());
            (ol.rebuild)(Some(at));
        });
    }
    {
        let ol = ol.clone();
        remove_sep.connect_clicked(move |_| {
            let Some(i) = ol.selected() else { return };
            if ol.order.borrow().get(i).map(String::as_str) != Some(actions::SEPARATOR) {
                return;
            }
            ol.order.borrow_mut().remove(i);
            save(&ol.order.borrow());
            let last = ol.order.borrow().len().saturating_sub(1);
            (ol.rebuild)(Some(i.min(last)));
        });
    }
    {
        let refresh = refresh.clone();
        reset.connect_clicked(move |_| {
            config::update(|s| if bg { s.menu_order_bg.clear() } else { s.menu_order.clear() });
            refresh();
        });
    }
    {
        let (order, remove_sep) = (ol.order.clone(), remove_sep.clone());
        ol.list.connect_row_selected(move |_, row| {
            let sep = row.and_then(|r| order.borrow().get(r.index() as usize).cloned()).as_deref() == Some(actions::SEPARATOR);
            remove_sep.set_sensitive(sep);
        });
    }
    outer.add(&button_row(&[&up, &down, &add_sep, &remove_sep], &[&reset]));
    // The View submenu keeps its own order; its entries can be hidden.
    if bg {
        let hb = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        hb.add(&gtk::Label::new(Some(&tr("View submenu:"))));
        let flow = gtk::FlowBox::new();
        flow.set_selection_mode(gtk::SelectionMode::None);
        flow.set_max_children_per_line(4);
        flow.set_column_spacing(12);
        flow.set_hexpand(true);
        for key in actions::VIEW_SUBMENU {
            let label = labels.iter().find(|(k, _)| k == key).map_or(key.to_string(), |(_, l)| tr(l));
            // Just what follows "View › ", in any language.
            let l = gtk::Label::new(Some(label.rsplit(" › ").next().unwrap_or(&label)));
            flow.add(&shown_check(l.upcast_ref(), key, |s| &mut s.hidden_menu_items, || {}));
        }
        hb.add(&flow);
        outer.add(&hb);
    }
    (outer, refresh)
}

/// The side pane's Computer entries: which are shown (their checkbox) and
/// in which order (dragging their handle, the arrows, or dragging them in
/// the side pane itself). Returns the editor and a function that reloads
/// it (they may have been dragged in the side pane meanwhile).
fn places_editor(places: impl Fn() -> Vec<crate::sidebar::Place> + 'static) -> (gtk::Box, Rc<dyn Fn()>) {
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let shown: Rc<RefCell<Vec<crate::sidebar::Place>>> = Rc::default();
    let row = {
        let shown = shown.clone();
        move |k: &str| -> gtk::Widget {
            let hb = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            if let Some(p) = shown.borrow().iter().find(|p| p.key == k) {
                if let Some(icon) = &p.icon {
                    hb.add(&gtk::Image::from_gicon(icon, gtk::IconSize::Menu));
                }
                hb.add(&gtk::Label::new(Some(&p.label)));
            }
            // In a box: the rest of the row selects it, not the checkbox.
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            row.add(&shown_check(hb.upcast_ref(), k, |s| &mut s.hidden_places, crate::sidebar::refresh_all));
            row.upcast()
        }
    };
    let ol = order_list(PLACE_ROW_TARGET, row, |o| {
        let o = o.to_vec();
        config::update(|s| s.places_order = o);
        crate::sidebar::refresh_all();
    });
    outer.add(&ol.widget);
    let refresh: Rc<dyn Fn()> = {
        let ol = ol.clone();
        Rc::new(move || {
            let mut p = places();
            let mut seen = vec![];
            p.retain(|p| {
                let new = !seen.contains(&p.key);
                seen.push(p.key.clone());
                new
            });
            // Unchanged: the rows stay. (The dialog gets the focus back
            // during a drag of a row on Wayland, and rebuilt, the dragged
            // row would be gone before it gives its data.)
            let keys: Vec<String> = p.iter().map(|p| p.key.clone()).collect();
            let same_labels = shown.borrow().iter().map(|p| &p.label).eq(p.iter().map(|p| &p.label));
            if *ol.order.borrow() == keys && same_labels {
                return;
            }
            *ol.order.borrow_mut() = keys;
            *shown.borrow_mut() = p;
            let sel = ol.selected();
            (ol.rebuild)(sel);
        })
    };
    refresh();
    let (up, down) = ol.arrows();
    let reset = gtk::Button::with_mnemonic(&tr("Reset _Order"));
    reset.set_tooltip_text(Some(&tr("Put every entry back in its usual place (which entries are hidden stays as it is)")));
    {
        let refresh = refresh.clone();
        reset.connect_clicked(move |_| {
            config::update(|s| s.places_order.clear());
            crate::sidebar::refresh_all();
            refresh();
        });
    }
    outer.add(&button_row(&[&up, &down], &[&reset]));
    (outer, refresh)
}

/// Every checksum with a check box (offered or not) in the order the
/// Checksums tab shows them, to drag or move with the arrows.
fn checksums_editor() -> gtk::Box {
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let row = |id: &str| -> gtk::Widget {
        let name = crate::checksums::Algo::from_id(id).map_or(id, |a| a.name());
        // In a box: the rest of the row selects it, not the checkbox.
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.add(&shown_check(gtk::Label::new(Some(name)).upcast_ref(), id, |s| &mut s.hidden_checksums, || {}));
        row.upcast()
    };
    let ol = order_list(CHECKSUM_ROW_TARGET, row, |o| {
        let o = o.to_vec();
        config::update(|s| s.checksum_order = o);
    });
    *ol.order.borrow_mut() = config::get().checksum_order;
    (ol.rebuild)(None);
    outer.add(&ol.widget);
    let (up, down) = ol.arrows();
    let reset = gtk::Button::with_mnemonic(&tr("Reset _Order"));
    reset.set_tooltip_text(Some(&tr("Put every checksum back in its usual place (which are offered stays as it is)")));
    {
        let ol = ol.clone();
        reset.connect_clicked(move |_| {
            let order: Vec<String> = crate::checksums::ALL.iter().map(|a| a.id().to_string()).collect();
            config::update(|s| s.checksum_order = order.clone());
            *ol.order.borrow_mut() = order;
            (ol.rebuild)(None);
        });
    }
    outer.add(&button_row(&[&up, &down], &[&reset]));
    outer
}

/// The Properties window's tabs in their order (the same order dragging its
/// tabs gives), each with a checkbox to show or hide it; Basic always shows.
fn props_tabs_editor() -> gtk::Box {
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let row = |id: &str| -> gtk::Widget {
        let label = gtk::Label::new(Some(&crate::props::tab_title(id)));
        // In a box: the rest of the row selects it, not the checkbox.
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let check = shown_check(label.upcast_ref(), id, |s| &mut s.hidden_props_tabs, || {});
        if id == "basic" {
            check.set_active(true);
            check.set_sensitive(false);
            check.set_tooltip_text(Some(&tr("Always shown, so the window has a tab")));
        }
        row.add(&check);
        row.upcast()
    };
    let ol = order_list(PROPS_TAB_ROW_TARGET, row, |o| {
        let o = o.to_vec();
        config::update(|s| s.props_tab_order = o);
    });
    *ol.order.borrow_mut() = config::get().props_tab_order;
    (ol.rebuild)(None);
    // All seven at once, never scrolled (however little room there is):
    // the checksums list below scrolls instead.
    ol.widget.set_vexpand(false);
    ol.widget.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Never);
    outer.add(&ol.widget);
    let (up, down) = ol.arrows();
    let reset = gtk::Button::with_mnemonic(&tr("Reset O_rder"));
    reset.set_tooltip_text(Some(&tr("Put every tab back in its usual place (which are shown stays as it is)")));
    {
        let ol = ol.clone();
        reset.connect_clicked(move |_| {
            let order: Vec<String> = config::PROPS_TABS.iter().map(|t| t.to_string()).collect();
            config::update(|s| s.props_tab_order = order.clone());
            *ol.order.borrow_mut() = order;
            (ol.rebuild)(None);
        });
    }
    outer.add(&button_row(&[&up, &down], &[&reset]));
    outer
}

/// List of the GTK bookmarks (shared with Caja and file choosers) with rename,
/// reorder, add and remove. Changes are written straight to the bookmarks file.
pub(super) fn bookmark_editor(parent: &gtk::Window) -> gtk::Box {
    let store = gtk::ListStore::new(&[String::static_type(), String::static_type()]);
    for (uri, label) in util::read_bookmarks() {
        store.insert_with_values(None, &[(0, &label), (1, &uri)]);
    }
    let tv = gtk::TreeView::with_model(&store);
    tv.set_reorderable(true);
    let name_r = gtk::CellRendererText::new();
    name_r.set_editable(true);
    let c1 = gtk::TreeViewColumn::new();
    c1.set_title(&tr("Name"));
    CellLayoutExt::pack_start(&c1, &name_r, true);
    CellLayoutExt::add_attribute(&c1, &name_r, "text", 0);
    c1.set_expand(true);
    let c2 = gtk::TreeViewColumn::new();
    let loc_r = gtk::CellRendererText::new();
    loc_r.set_property("ellipsize", pango::EllipsizeMode::Middle);
    c2.set_title(&tr("Location"));
    CellLayoutExt::pack_start(&c2, &loc_r, true);
    // Kept as URLs (as the bookmarks file has them), shown as people know
    // them: a folder's path, another place's address without its escapes.
    CellLayoutExt::set_cell_data_func(
        &c2,
        &loc_r,
        Some(Box::new(|_, cell, m, it| cell.set_property("text", bookmark_location(&m.gv::<String>(it, 1))))),
    );
    c2.set_expand(true);
    tv.append_column(&c1);
    tv.append_column(&c2);
    let save = {
        let store = store.clone();
        move || {
            let list: Vec<(String, String)> = store.rows().iter().map(|it| (store.gv::<String>(it, 1), store.gv::<String>(it, 0))).collect();
            let _ = util::edit_bookmarks(|b| *b = list);
        }
    };
    let save = Rc::new(save);
    {
        let store = store.clone();
        let save = save.clone();
        name_r.connect_edited(move |_, path, text| {
            if let Some(it) = store.iter(&path) {
                store.set(&it, &[(0, &text.to_string())]);
                save();
            }
        });
    }
    {
        let save = save.clone();
        store.connect_row_deleted(move |_, _| save());
    }
    let sw = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    sw.set_shadow_type(gtk::ShadowType::In);
    sw.add(&tv);
    sw.set_vexpand(true);
    let rm = gtk::Button::with_mnemonic(&tr("_Remove"));
    {
        let tv = tv.clone();
        let store = store.clone();
        rm.connect_clicked(move |_| {
            if let Some((_, it)) = tv.selection().selected() {
                store.remove(&it);
            }
        });
    }
    let hint = gtk::Label::new(Some(&tr("Drag to reorder. Double-click a name to rename it.")));
    hint.style_context().add_class("dim-label");
    hint.set_xalign(0.0);
    let add = gtk::Button::with_mnemonic(&tr("_Add…"));
    {
        let store = store.clone();
        let parent = parent.clone();
        add.connect_clicked(move |_| {
            let fc = gtk::FileChooserDialog::with_buttons(
                Some(&tr("Add Bookmark")),
                Some(&parent),
                gtk::FileChooserAction::SelectFolder,
                &[(tr("_Cancel").as_str(), gtk::ResponseType::Cancel), (tr("_Add").as_str(), gtk::ResponseType::Accept)],
            );
            if fc.run() == gtk::ResponseType::Accept {
                if let Some(f) = fc.file() {
                    let uri = f.uri().to_string();
                    if let Ok(true) = util::add_bookmark(&uri) {
                        let label = f.basename().map(|b| b.to_string_lossy().to_string()).unwrap_or_else(|| uri.clone());
                        store.insert_with_values(None, &[(0, &label), (1, &uri)]);
                    }
                }
            }
            fc.close();
        });
    }
    let bx = gtk::Box::new(gtk::Orientation::Vertical, 6);
    bx.pack_start(&sw, true, true, 0);
    let hb = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    hb.pack_start(&hint, true, true, 0);
    hb.pack_end(&rm, false, false, 0);
    hb.pack_end(&add, false, false, 0);
    bx.pack_start(&hb, false, false, 0);
    bx
}

/// How a bookmark's location shows: "/home/user/Backups" for
/// file:///home/user/Backups, "smb://server/My Share" for
/// smb://server/My%20Share.
fn bookmark_location(uri: &str) -> String {
    let f = gio::File::for_uri(uri);
    match f.path() {
        Some(p) if f.has_uri_scheme("file") => p.to_string_lossy().to_string(),
        _ => glib::Uri::unescape_string(uri, None::<&str>).map_or_else(|| uri.to_string(), |u| u.to_string()),
    }
}

/// Every action with its shortcuts, grouped. Double-click (or Change…) to
/// record a new shortcut; Clear removes it, Reset brings back the default.
fn keybindings_editor(parent: &gtk::Window, app: &gtk::Application) -> gtk::Box {
    // label, shortcuts, action ("" for group rows), weight
    let store = gtk::TreeStore::new(&[String::static_type(), String::static_type(), String::static_type(), i32::static_type()]);
    let refill = {
        let store = store.clone();
        move || {
            store.clear();
            let mut group: Option<(&str, gtk::TreeIter)> = None;
            for b in keys::BINDINGS {
                if group.as_ref().map(|(g, _)| *g) != Some(b.group) {
                    let it = store.insert_with_values(None, None, &[(0, &tr(b.group)), (1, &""), (2, &""), (3, &700)]);
                    group = Some((b.group, it));
                }
                let parent = group.as_ref().map(|(_, it)| *it);
                // Changed shortcuts are shown in bold.
                let weight = if keys::is_default(b.action) { 400 } else { 700 };
                let current = keys::current(b.action);
                let text = if current.is_empty() { "—".to_string() } else { keys::labels(&current) };
                store.insert_with_values(parent.as_ref(), None, &[(0, &tr(b.label)), (1, &text), (2, &b.action), (3, &weight)]);
            }
        }
    };
    refill();
    let tv = gtk::TreeView::with_model(&store);
    tv.set_show_expanders(false);
    tv.set_level_indentation(0);
    tv.set_enable_search(true);
    tv.set_search_column(0);
    for (i, title) in [n_("Action"), n_("Shortcut")].into_iter().enumerate() {
        let r = gtk::CellRendererText::new();
        let c = gtk::TreeViewColumn::new();
        c.set_title(&tr(title));
        c.set_expand(true);
        CellLayoutExt::pack_start(&c, &r, true);
        CellLayoutExt::add_attribute(&c, &r, "text", i as i32);
        CellLayoutExt::add_attribute(&c, &r, "weight", 3);
        tv.append_column(&c);
    }
    tv.expand_all();
    let selected = {
        let tv = tv.clone();
        move || -> Option<String> {
            let (m, it) = tv.selection().selected()?;
            let action: String = m.value(&it, 2).get().ok()?;
            (!action.is_empty()).then_some(action)
        }
    };
    let after = {
        let (refill, tv, selected) = (refill.clone(), tv.clone(), selected.clone());
        move || {
            let keep = selected();
            refill();
            tv.expand_all();
            // Re-select the row that was changed.
            if let Some(action) = keep {
                let i = keys::BINDINGS.iter().position(|b| b.action == action).unwrap_or(0);
                let group_idx = keys::BINDINGS[..=i].iter().fold((0, ""), |(n, g), b| if b.group != g { (n + 1, b.group) } else { (n, g) }).0 - 1;
                let in_group = keys::BINDINGS[..i].iter().filter(|b| b.group == keys::BINDINGS[i].group).count();
                let path = gtk::TreePath::from_indicesv(&[group_idx, in_group as i32]);
                tv.selection().select_path(&path);
                tv.scroll_to_cell(Some(&path), None::<&gtk::TreeViewColumn>, false, 0.0, 0.0);
            }
        }
    };
    let after = Rc::new(after);
    let change = {
        let (parent, app, after, selected) = (parent.clone(), app.clone(), after.clone(), selected.clone());
        move || {
            let Some(action) = selected() else { return };
            let Some(b) = keys::BINDINGS.iter().find(|b| b.action == action) else { return };
            let Some(accel) = capture_shortcut(&parent, &tr(b.label)) else { return };
            if let Some(other) = keys::used_by(&accel, &action) {
                let q = gtk::MessageDialog::new(
                    Some(&parent),
                    gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
                    gtk::MessageType::Question,
                    gtk::ButtonsType::None,
                    &fill(&tr("“{shortcut}” is already the shortcut for “{action}”"), &[("shortcut", &keys::labels(std::slice::from_ref(&accel))), ("action", &tr(other.label))]),
                );
                q.set_secondary_text(Some(&fill(&tr("Use it for “{action}” instead? “{other}” will no longer have it."), &[("action", &tr(b.label)), ("other", &tr(other.label))])));
                q.add_button(&tr("_Cancel"), gtk::ResponseType::Cancel);
                q.add_button(&tr("_Reassign"), gtk::ResponseType::Accept);
                let ok = q.run() == gtk::ResponseType::Accept;
                q.close();
                if !ok {
                    return;
                }
                let want = gtk::accelerator_parse(&accel);
                let rest: Vec<String> = keys::current(other.action).into_iter().filter(|a| gtk::accelerator_parse(a) != want).collect();
                keys::set(&app, other.action, Some(rest));
            }
            keys::set(&app, &action, Some(vec![accel]));
            after();
        }
    };
    let change = Rc::new(change);
    let btn_change = gtk::Button::with_mnemonic(&tr("C_hange…"));
    let btn_clear = gtk::Button::with_mnemonic(&tr("C_lear"));
    btn_clear.set_tooltip_text(Some(&tr("Remove the shortcut")));
    let btn_reset = gtk::Button::with_mnemonic(&tr("R_eset"));
    btn_reset.set_tooltip_text(Some(&tr("Go back to the default shortcut")));
    let btn_reset_all = gtk::Button::with_mnemonic(&tr("Reset _All"));
    btn_reset_all.set_tooltip_text(Some(&tr("Go back to the default shortcuts for every action")));
    {
        let change = change.clone();
        btn_change.connect_clicked(move |_| change());
    }
    {
        let change = change.clone();
        tv.connect_row_activated(move |_, _, _| change());
    }
    for (btn, clear) in [(&btn_clear, true), (&btn_reset, false)] {
        let (app, after, selected) = (app.clone(), after.clone(), selected.clone());
        btn.connect_clicked(move |_| {
            if let Some(action) = selected() {
                keys::set(&app, &action, if clear { Some(vec![]) } else { None });
                after();
            }
        });
    }
    {
        let (app, after) = (app.clone(), after.clone());
        btn_reset_all.connect_clicked(move |_| {
            keys::reset_all(&app);
            after();
        });
    }
    let sync = {
        let (b1, b2, b3) = (btn_change.clone(), btn_clear.clone(), btn_reset.clone());
        move |action: Option<String>| {
            b1.set_sensitive(action.is_some());
            b2.set_sensitive(action.as_deref().is_some_and(|a| !keys::current(a).is_empty()));
            b3.set_sensitive(action.as_deref().is_some_and(|a| !keys::is_default(a)));
        }
    };
    sync(None);
    {
        let selected = selected.clone();
        tv.selection().connect_changed(move |_| sync(selected()));
    }
    let buttons = button_row(&[&btn_change, &btn_clear, &btn_reset], &[&btn_reset_all]);
    let hint = gtk::Label::new(Some(&tr("Double-click an action to give it a new shortcut. Changed shortcuts are shown in bold.")));
    hint.set_xalign(0.0);
    hint.set_line_wrap(true);
    hint.style_context().add_class("dim-label");
    let frame = gtk::Frame::new(None);
    frame.add(&tv);
    let bx = gtk::Box::new(gtk::Orientation::Vertical, 6);
    bx.add(&buttons);
    bx.add(&hint);
    bx.pack_start(&frame, true, true, 0);
    bx
}

/// Waits for a key combination; None if cancelled with Escape or Cancel.
fn capture_shortcut(parent: &gtk::Window, label: &str) -> Option<String> {
    let d = gtk::Dialog::with_buttons(Some(&tr("Set Shortcut")), Some(parent), gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT, &[(tr("_Cancel").as_str(), gtk::ResponseType::Cancel)]);
    let msg = gtk::Label::new(None);
    // TRANSLATORS: keep <b>{action}</b>: the action's name is shown in bold.
    msg.set_markup(&fill(&tr("Press the new shortcut for <b>{action}</b>,\nor Escape to cancel."), &[("action", &glib::markup_escape_text(label))]));
    msg.set_justify(gtk::Justification::Center);
    let note = gtk::Label::new(None);
    note.style_context().add_class("dim-label");
    let bx = d.content_area();
    bx.set_spacing(8);
    bx.set_border_width(18);
    bx.add(&msg);
    bx.add(&note);
    let result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    {
        let result = result.clone();
        d.connect_key_press_event(move |d, ev| {
            if ev.is_modifier() {
                return glib::Propagation::Stop;
            }
            let key = ev.keyval().to_lower();
            let mods = ev.state() & gtk::accelerator_get_default_mod_mask();
            if key == gdk::keys::constants::Escape && mods.is_empty() {
                d.response(gtk::ResponseType::Cancel);
                return glib::Propagation::Stop;
            }
            // A plain character key would get in the way of typing names.
            let typing = key.to_unicode().is_some_and(|c| !c.is_control())
                && !mods.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::MOD1_MASK | gdk::ModifierType::SUPER_MASK);
            if typing || !gtk::accelerator_valid(*key, mods) {
                note.set_text(&tr("That key needs Ctrl, Alt or Super with it."));
                return glib::Propagation::Stop;
            }
            *result.borrow_mut() = Some(gtk::accelerator_name(*key, mods).map(|n| n.to_string()).unwrap_or_default());
            d.response(gtk::ResponseType::Ok);
            glib::Propagation::Stop
        });
    }
    d.show_all();
    let ok = d.run() == gtk::ResponseType::Ok;
    d.close();
    let r = result.borrow_mut().take();
    if ok { r.filter(|a| !a.is_empty()) } else { None }
}

/// List of custom thumbnailers (commands for file types the built-in image
/// thumbnailer doesn't do) with Add / Edit / Remove; saved straight away.
fn thumbnailers_editor(parent: &gtk::Window) -> gtk::Box {
    let spec = ListSpec {
        columns: const { &[n_("Name"), n_("File types"), n_("Command")] },
        // Ones whose program is missing are skipped when making thumbnails.
        row: |t: &thumbnailers::Thumbnailer| {
            let name = if thumbnailers::installed(t) { t.name.clone() } else { fill(&tr("{name} (not installed)"), &[("name", &t.name)]) };
            vec![name, t.types.clone(), t.command.clone()]
        },
        blank: thumbnailers::Thumbnailer::default,
        edit: edit_thumbnailer,
        add_title: n_("Add Thumbnailer"),
        edit_title: n_("Edit Thumbnailer"),
        reorder: false,
        min_height: 90,
    };
    // Saved, and thumbnails made again in the open folders with the new list.
    let bx = list_editor(parent, thumbnailers::load(), spec, |list| {
        thumbnailers::save(list);
        icons::reload_thumbnailers();
        for_all_panes(|p| p.reload());
    });
    bx.add(&hint_label(&tr("Custom thumbnailers make thumbnails for other file types, e.g. videos with ffmpeg. The first one whose file types match and whose program is installed is used. File types none of them handles use the thumbnailers installed on the system: PDF with Evince, office documents, audio cover art and so on.")));
    bx
}

/// What list_editor edits and how.
struct ListSpec<T> {
    columns: &'static [&'static str],
    /// The text of each column for an item.
    row: fn(&T) -> Vec<String>,
    /// What Add starts from.
    blank: fn() -> T,
    /// The dialog that changes an item (title, item): None if cancelled.
    edit: fn(&gtk::Window, &str, &T) -> Option<T>,
    add_title: &'static str,
    edit_title: &'static str,
    /// Up and down buttons, where the order counts.
    reorder: bool,
    min_height: i32,
}

/// A list edited in Preferences: a table with Add, Edit and Remove (and up
/// and down), every change given to `save` straight away. More (a hint)
/// can be added to the box it gives.
fn list_editor<T: Clone + 'static>(parent: &gtk::Window, items: Vec<T>, spec: ListSpec<T>, save: impl Fn(&[T]) + 'static) -> gtk::Box {
    let list = Rc::new(RefCell::new(items));
    let store = gtk::ListStore::new(&vec![String::static_type(); spec.columns.len()]);
    let refill = {
        let (store, list) = (store.clone(), list.clone());
        move || {
            store.clear();
            for item in list.borrow().iter() {
                let texts = (spec.row)(item);
                let values: Vec<(u32, &dyn ToValue)> = texts.iter().enumerate().map(|(i, t)| (i as u32, t as &dyn ToValue)).collect();
                store.insert_with_values(None, &values);
            }
        }
    };
    refill();
    let tv = gtk::TreeView::with_model(&store);
    for (i, title) in spec.columns.iter().enumerate() {
        let r = gtk::CellRendererText::new();
        r.set_property("ellipsize", pango::EllipsizeMode::End);
        let c = gtk::TreeViewColumn::new();
        c.set_title(&tr(title));
        c.set_expand(true);
        c.set_resizable(true);
        CellLayoutExt::pack_start(&c, &r, true);
        CellLayoutExt::add_attribute(&c, &r, "text", i as i32);
        tv.append_column(&c);
    }
    let sw = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    sw.set_shadow_type(gtk::ShadowType::In);
    sw.set_min_content_height(spec.min_height);
    sw.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    sw.add(&tv);
    let selected = {
        let tv = tv.clone();
        move || tv.selection().selected().and_then(|(m, it)| m.path(&it)?.indices().first().map(|&i| i as usize))
    };
    // Saved and shown again, with `select` selected.
    let changed = Rc::new({
        let (list, tv) = (list.clone(), tv.clone());
        move |select: Option<usize>| {
            save(&list.borrow());
            refill();
            if let Some(i) = select {
                tv.selection().select_path(&gtk::TreePath::from_indicesv(&[i as i32]));
            }
        }
    });
    let add = gtk::Button::with_mnemonic(&tr("_Add…"));
    let edit = gtk::Button::with_mnemonic(&tr("_Edit…"));
    let remove = gtk::Button::with_mnemonic(&tr("_Remove"));
    let up = gtk::Button::from_icon_name(Some("go-up-symbolic"), gtk::IconSize::Button);
    up.set_tooltip_text(Some(&tr("Move up")));
    let down = gtk::Button::from_icon_name(Some("go-down-symbolic"), gtk::IconSize::Button);
    down.set_tooltip_text(Some(&tr("Move down")));
    {
        let (list, changed, parent) = (list.clone(), changed.clone(), parent.clone());
        add.connect_clicked(move |_| {
            if let Some(item) = (spec.edit)(&parent, &tr(spec.add_title), &(spec.blank)()) {
                list.borrow_mut().push(item);
                let n = list.borrow().len();
                changed(Some(n - 1));
            }
        });
    }
    let do_edit = Rc::new({
        let (list, changed, parent, selected) = (list.clone(), changed.clone(), parent.clone(), selected.clone());
        move || {
            let Some(i) = selected() else { return };
            let cur = list.borrow()[i].clone();
            if let Some(item) = (spec.edit)(&parent, &tr(spec.edit_title), &cur) {
                list.borrow_mut()[i] = item;
                changed(Some(i));
            }
        }
    });
    {
        let do_edit = do_edit.clone();
        edit.connect_clicked(move |_| do_edit());
    }
    tv.connect_row_activated(move |_, _, _| do_edit());
    {
        let (list, changed, selected) = (list.clone(), changed.clone(), selected.clone());
        remove.connect_clicked(move |_| {
            if let Some(i) = selected() {
                list.borrow_mut().remove(i);
                changed(None);
            }
        });
    }
    for (btn, delta) in [(&up, -1i32), (&down, 1)] {
        let (list, changed, selected) = (list.clone(), changed.clone(), selected.clone());
        btn.connect_clicked(move |_| {
            let Some(i) = selected() else { return };
            let j = i as i32 + delta;
            if j < 0 || j as usize >= list.borrow().len() {
                return;
            }
            list.borrow_mut().swap(i, j as usize);
            changed(Some(j as usize));
        });
    }
    // Buttons that need a selected item.
    let sync = {
        let (edit, remove, up, down, list) = (edit.clone(), remove.clone(), up.clone(), down.clone(), list.clone());
        move |sel: Option<usize>| {
            let n = list.borrow().len();
            edit.set_sensitive(sel.is_some());
            remove.set_sensitive(sel.is_some());
            up.set_sensitive(sel.is_some_and(|i| i > 0));
            down.set_sensitive(sel.is_some_and(|i| i + 1 < n));
        }
    };
    sync(None);
    tv.selection().connect_changed(move |_| sync(selected()));
    let arrows = [&up, &down];
    let buttons = button_row(&[&add, &edit, &remove], if spec.reorder { &arrows } else { &[] });
    let bx = gtk::Box::new(gtk::Orientation::Vertical, 6);
    bx.add(&sw);
    bx.add(&buttons);
    bx
}

/// Dialog to create or change a custom thumbnailer; None if cancelled.
fn edit_thumbnailer(parent: &gtk::Window, title: &str, cur: &thumbnailers::Thumbnailer) -> Option<thumbnailers::Thumbnailer> {
    let (d, grid) = edit_dialog(parent, title);
    let entry = |text: &str, placeholder: &str| {
        let e = gtk::Entry::new();
        e.set_text(text);
        e.set_placeholder_text(Some(placeholder));
        e.set_activates_default(true);
        e.set_hexpand(true);
        e
    };
    let name = entry(&cur.name, &tr("e.g. Videos"));
    let types = entry(&cur.types, &tr("e.g. video/*"));
    let command = entry(&cur.command, &tr("e.g. ffmpegthumbnailer -i %i -o %o -s %s -f"));
    command.set_width_chars(56);
    let help = gtk::Label::new(Some(&tr("File types are MIME types, separated by “;” (“*” matches anything). In the command, %i is the file, %o the image to write (PNG or JPEG) and %s the size in pixels. It runs with sh and may take up to 30 seconds.")));
    help.set_line_wrap(true);
    help.set_max_width_chars(56);
    help.set_xalign(0.0);
    help.style_context().add_class("dim-label");
    let rows: [(&str, &gtk::Widget); 4] = [(n_("_Name:"), name.upcast_ref()), (n_("File _types:"), types.upcast_ref()), (n_("_Command:"), command.upcast_ref()), ("", help.upcast_ref())];
    for (i, (t, w)) in rows.iter().enumerate() {
        let l = gtk::Label::with_mnemonic(&tr(t));
        l.set_xalign(0.0);
        l.set_mnemonic_widget(Some(*w));
        grid.attach(&l, 0, i as i32, 1, 1);
        grid.attach(*w, 1, i as i32, 1, 1);
    }
    d.content_area().add(&grid);
    let validate = {
        let (d, types, command) = (d.clone(), types.clone(), command.clone());
        move || d.set_response_sensitive(gtk::ResponseType::Ok, !types.text().trim().is_empty() && !command.text().trim().is_empty())
    };
    validate();
    for e in [&types, &command] {
        let v = validate.clone();
        e.connect_changed(move |_| v());
    }
    d.show_all();
    let ok = d.run() == gtk::ResponseType::Ok;
    let types_text = types.text().trim().to_string();
    let result = thumbnailers::Thumbnailer {
        name: Some(name.text().trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| types_text.clone()),
        types: types_text,
        command: command.text().trim().to_string(),
        system: false,
    };
    d.close();
    ok.then_some(result)
}

/// List of custom context menu actions with Add / Edit / Remove / reorder;
/// every change is saved straight away.
fn custom_actions_editor(parent: &gtk::Window) -> gtk::Box {
    let spec = ListSpec {
        columns: const { &[n_("Name"), n_("Command")] },
        row: |a: &actions::CustomAction| vec![a.name.clone(), a.command.clone()],
        blank: || actions::CustomAction { id: actions::new_id(), files: true, folders: true, ..Default::default() },
        edit: edit_custom_action,
        add_title: n_("Add Custom Action"),
        edit_title: n_("Edit Custom Action"),
        reorder: true,
        min_height: 130,
    };
    let bx = list_editor(parent, actions::load(), spec, actions::save);
    let hint = hint_label(&tr("Commands run in the current folder. %f selected file, %F all selected files, %n / %N their names, %d current folder."));
    // The shell commands run with, as `<shell> -c <command>`.
    let shell = gtk::Entry::new();
    shell.set_text(&config::get().action_shell);
    shell.set_placeholder_text(Some("sh"));
    shell.set_width_chars(16);
    shell.set_tooltip_text(Some(&tr("A shell that accepts -c, such as sh, bash, zsh or fish, or its full path. Empty means sh.")));
    let check_shell = |e: &gtk::Entry| {
        if actions::shell_found(&actions::shell()) {
            e.style_context().remove_class("error");
        } else {
            e.style_context().add_class("error");
        }
    };
    check_shell(&shell);
    shell.connect_changed(move |e| {
        let v = e.text().trim().to_string();
        config::update(|s| s.action_shell = v);
        check_shell(e);
    });
    let shell_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let shell_label = gtk::Label::with_mnemonic(&tr("Run commands _with:"));
    shell_label.set_mnemonic_widget(Some(&shell));
    shell_row.add(&shell_label);
    shell_row.add(&shell);
    bx.add(&shell_row);
    bx.add(&hint);
    bx
}

/// Dialog to create or change a custom action; None if cancelled.
fn edit_custom_action(parent: &gtk::Window, title: &str, cur: &actions::CustomAction) -> Option<actions::CustomAction> {
    let (d, grid) = edit_dialog(parent, title);
    let label = |t: &str| {
        let l = gtk::Label::with_mnemonic(&tr(t));
        l.set_xalign(0.0);
        l
    };
    let name = gtk::Entry::new();
    name.set_text(&cur.name);
    name.set_activates_default(true);
    name.set_hexpand(true);
    let command = gtk::Entry::new();
    command.set_text(&cur.command);
    command.set_activates_default(true);
    command.set_width_chars(40);
    command.set_placeholder_text(Some(&tr("e.g. gimp %F")));
    let help = gtk::Label::new(Some(&tr("%f first selected file, %F all selected files, %n / %N their names, %d current folder, %% a % sign. Paths are quoted for you.")));
    help.set_line_wrap(true);
    help.set_max_width_chars(50);
    help.set_xalign(0.0);
    help.style_context().add_class("dim-label");
    let files = gtk::CheckButton::with_mnemonic(&tr("_Files"));
    files.set_active(cur.files);
    let folders = gtk::CheckButton::with_mnemonic(&tr("F_olders"));
    folders.set_active(cur.folders);
    let background = gtk::CheckButton::with_mnemonic(&tr("Empty space in a _folder"));
    background.set_active(cur.background);
    let show = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    show.add(&files);
    show.add(&folders);
    show.add(&background);
    // Which files: by type, by name pattern, or both (either matches).
    let chosen: Vec<&str> = cur.types.split(';').map(str::trim).collect();
    let types = gtk::FlowBox::new();
    types.set_selection_mode(gtk::SelectionMode::None);
    types.set_max_children_per_line(3);
    types.set_column_spacing(12);
    let type_checks: Vec<(&'static str, gtk::CheckButton)> = actions::FILE_TYPES
        .iter()
        .map(|(id, label)| {
            let c = gtk::CheckButton::with_label(&tr(label));
            c.set_active(chosen.contains(id));
            types.add(&c);
            (*id, c)
        })
        .collect();
    let patterns = gtk::Entry::new();
    patterns.set_text(&cur.patterns);
    patterns.set_activates_default(true);
    patterns.set_placeholder_text(Some(&tr("e.g. *.srt; *.nfo")));
    let which = gtk::Label::new(Some(&tr("A file needs one of the checked types or a name matching a pattern; with neither set, the action is offered for any file. Folders only go by the patterns.")));
    which.set_line_wrap(true);
    which.set_max_width_chars(50);
    which.set_xalign(0.0);
    which.style_context().add_class("dim-label");
    // File types only concern files.
    types.set_sensitive(cur.files);
    {
        let types = types.clone();
        files.connect_toggled(move |c| types.set_sensitive(c.is_active()));
    }
    let rows: [(&str, &gtk::Widget); 7] = [
        (n_("_Name:"), name.upcast_ref()),
        (n_("_Command:"), command.upcast_ref()),
        ("", help.upcast_ref()),
        (n_("Show for:"), show.upcast_ref()),
        (n_("File types:"), types.upcast_ref()),
        (n_("Name _patterns:"), patterns.upcast_ref()),
        ("", which.upcast_ref()),
    ];
    for (i, (t, w)) in rows.iter().enumerate() {
        let l = label(t);
        l.set_mnemonic_widget(Some(*w));
        grid.attach(&l, 0, i as i32, 1, 1);
        grid.attach(*w, 1, i as i32, 1, 1);
    }
    d.content_area().add(&grid);
    // Saving needs a name, a command and somewhere to show up.
    let validate = {
        let (d, name, command, files, folders, background) = (d.clone(), name.clone(), command.clone(), files.clone(), folders.clone(), background.clone());
        move || {
            let ok = !name.text().trim().is_empty()
                && !command.text().trim().is_empty()
                && (files.is_active() || folders.is_active() || background.is_active());
            d.set_response_sensitive(gtk::ResponseType::Ok, ok);
        }
    };
    validate();
    for e in [&name, &command] {
        let v = validate.clone();
        e.connect_changed(move |_| v());
    }
    for c in [&files, &folders, &background] {
        let v = validate.clone();
        c.connect_toggled(move |_| v());
    }
    d.show_all();
    let ok = d.run() == gtk::ResponseType::Ok;
    let result = actions::CustomAction {
        id: if cur.id.is_empty() { actions::new_id() } else { cur.id.clone() },
        name: name.text().trim().to_string(),
        command: command.text().trim().to_string(),
        files: files.is_active(),
        folders: folders.is_active(),
        background: background.is_active(),
        patterns: patterns.text().trim().to_string(),
        types: type_checks.iter().filter(|(_, c)| c.is_active()).map(|(id, _)| *id).collect::<Vec<_>>().join(";"),
    };
    d.close();
    ok.then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookmark_locations_shown_as_paths() {
        assert_eq!(bookmark_location("file:///home/user/Backups"), "/home/user/Backups");
        assert_eq!(bookmark_location("file:///home/user/My%20Files"), "/home/user/My Files");
        assert_eq!(bookmark_location("smb://server/My%20Share/"), "smb://server/My Share/");
        assert_eq!(bookmark_location("sftp://host/srv/data"), "sftp://host/srv/data");
    }
}
