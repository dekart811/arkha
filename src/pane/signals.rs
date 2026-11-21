//! The views' signals: what clicks, keys, drags and edits do.

use super::*;

impl Pane {
    pub(super) fn setup_signals(self: &Rc<Self>) {
        self.connect_general();
        self.connect_icon_view();
        self.connect_tree_view();
        self.connect_dnd();
        self.connect_rename();
        self.connect_typeahead();
    }

    /// Columns view, rename editor, search editor, info panel.
    fn connect_general(self: &Rc<Self>) {
        {
            let w = Rc::downgrade(self);
            self.strip_sw.connect_map(move |_| {
                if let Some(p) = w.upgrade() {
                    if p.columns_end_on_map.replace(false) {
                        p.scroll_columns_to_end();
                    }
                }
            });
        }
        let w = Rc::downgrade(self);
        // The query editor's rows are kept with the search; Reload (or Enter
        // in a row, or the subfolders box) searches again.
        {
            let w1 = w.clone();
            self.query_editor.connect_changed(move |criteria| {
                if let Some(p) = w1.upgrade() {
                    if let Some(q) = p.search.borrow_mut().as_mut() {
                        q.criteria = criteria;
                    }
                }
            });
            let w = w.clone();
            self.query_editor.connect_reload(move || {
                if let Some(p) = w.upgrade() {
                    p.reload();
                }
            });
        }
        // The rename editor sits over the item's name, also as the view
        // scrolls.
        {
            let w = w.clone();
            self.overlay.connect_get_child_position(move |_, child| {
                let p = w.upgrade()?;
                let ed = p.rename_editor.borrow().clone()?;
                if child != ed.frame.upcast_ref::<gtk::Widget>() {
                    return None;
                }
                if let Some(r) = p.rename_rect(&ed) {
                    p.rename_tries.set(0);
                    return Some(r);
                }
                // Its item isn't laid out yet (just added): out of sight
                // until it is (at its own size: smaller, GTK complains),
                // asked again shortly (for up to 2 s).
                if p.rename_tries.get() < 40 {
                    p.rename_tries.set(p.rename_tries.get() + 1);
                    let ov = p.overlay.clone();
                    glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || ov.queue_resize());
                }
                let (w, h) = (ed.frame.preferred_width().1.max(1), ed.frame.preferred_height().1.max(1));
                Some(gdk::Rectangle::new(-w - 100, -h - 100, w, h))
            });
            if let Some(adj) = self.icon_view.vadjustment() {
                let ov = self.overlay.clone();
                adj.connect_value_changed(move |_| ov.queue_resize());
            }
        }
        {
            let w = w.clone();
            self.tree_view.connect_key_press_event(move |_, ev| match w.upgrade() {
                Some(p) => p.columns_key(ev),
                None => glib::Propagation::Proceed,
            });
        }
        // Double-clicking the divider at a column header's right edge fits
        // the column to its contents, as on Windows. (GTK's own double-click
        // there only goes back to the column's natural width, which is its
        // minimum while long text is cut off with "…".)
        {
            let w = w.clone();
            self.tree_view.connect_button_press_event(move |_, ev| {
                if ev.button() == 1 && ev.event_type() == gdk::EventType::DoubleButtonPress {
                    if let Some(p) = w.upgrade() {
                        if let Some(col) = p.column_divider_at(ev.root()) {
                            p.fit_column(&col);
                        }
                    }
                }
                glib::Propagation::Proceed
            });
        }
        // The columns view: a right-click on the blank space beside the
        // columns opens the menu of the open folder (to create something in
        // it, say), a left-click there clears the selection.
        {
            let w = w.clone();
            self.strip_events.connect_button_press_event(move |_, ev| {
                let Some(p) = w.upgrade() else { return glib::Propagation::Proceed };
                if !p.columns_mode.get() || ev.event_type() != gdk::EventType::ButtonPress {
                    return glib::Propagation::Proceed;
                }
                match ev.button() {
                    1 => p.select_filter_paths(&[]),
                    3 => {
                        p.select_filter_paths(&[]);
                        p.emit(Event::Context { event: Some(std::ops::Deref::deref(ev).clone()), on_item: false });
                    }
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            });
        }
        // An empty open folder in the columns view says so, like the side
        // columns.
        {
            let w = w.clone();
            self.tree_view.connect_local("draw", true, move |vals| {
                let p = w.upgrade()?;
                if p.columns_mode.get() && !p.loading.get() && p.filter.iter_first().is_none() {
                    let v = vals[0].get::<gtk::TreeView>().ok()?;
                    let cr = vals[1].get::<gtk::cairo::Context>().ok()?;
                    crate::columns::draw_empty_hint(&v, &cr);
                }
                Some(false.to_value())
            });
        }
        // Dragging the info panel's divider: the width (shared by all
        // panes) is remembered, and the picture reloaded at the new size,
        // once the dragging stops.
        {
            let w = w.clone();
            let gen = Rc::new(Cell::new(0u64));
            self.info_paned.connect_position_notify(move |pn| {
                let Some(p) = w.upgrade() else { return };
                let total = pn.allocated_width();
                if !p.info.widget.is_visible() || total <= 1 {
                    return;
                }
                let width = total - pn.position();
                let g = gen.get() + 1;
                gen.set(g);
                let (w, gen) = (w.clone(), gen.clone());
                glib::timeout_add_local_once(Duration::from_millis(400), move || {
                    let Some(p) = w.upgrade() else { return };
                    if gen.get() != g || width == config::with(|s| s.info_panel_width) {
                        return;
                    }
                    config::update(|s| s.info_panel_width = width);
                    p.refresh_info();
                });
            });
        }
    }

    /// The icon views: selection, activation, clicks, context menu.
    fn connect_icon_view(self: &Rc<Self>) {
        let w = Rc::downgrade(self);
        let iv = &self.icon_view;
        {
            let w = w.clone();
            iv.connect_local("draw", true, move |vals| {
                let cr = vals[1].get::<gtk::cairo::Context>().ok()?;
                if let Some(p) = w.upgrade() {
                    p.draw_whole_labels(&cr);
                }
                Some(false.to_value())
            });
        }
        {
            // Columns fitted to the width (after the allocation: changing
            // the item width in it would start another).
            let w = w.clone();
            iv.connect_size_allocate(move |_, _| {
                let w = w.clone();
                glib::idle_add_local_once(move || {
                    if let Some(p) = w.upgrade() {
                        p.fit_icon_columns();
                    }
                });
            });
        }
        {
            let w = w.clone();
            iv.connect_selection_changed(move |iv| {
                // Selected names are drawn over the items below (see
                // draw_whole_labels): GTK only redraws the item itself.
                iv.queue_draw();
                if let Some(p) = w.upgrade() {
                    p.trim_rubber_band();
                    p.emit(Event::Selection);
                }
            });
        }
        {
            let w = w.clone();
            iv.connect_item_activated(move |_, _| {
                if let Some(p) = w.upgrade() {
                    p.emit(Event::Activate);
                }
            });
        }
        {
            let w = w.clone();
            iv.connect_button_press_event(move |iv, ev| {
                let Some(p) = w.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                p.keyboard_focus.set(false);
                // Clicks come in the bin window's coordinates.
                let (bx, by) = ev.position();
                let (ox, oy) = iv.convert_widget_to_bin_window_coords(0, 0);
                let (x, y) = (bx as i32 - ox, by as i32 - oy);
                let path = p.icon_hit(x, y);
                match ev.button() {
                    1 => p.icon_press(ev, iv.path_at_pos(bx as i32, by as i32).is_some(), path),
                    3 => {
                        iv.grab_focus();
                        p.context_click(
                            ev,
                            path.as_ref(),
                            |tp| iv.path_is_selected(tp),
                            |tp| {
                                iv.unselect_all();
                                iv.select_path(tp);
                                iv.set_cursor(tp, None::<&gtk::CellRenderer>, false);
                            },
                            || iv.unselect_all(),
                        )
                    }
                    2 => {
                        let row = path.as_ref().and_then(|tp| p.row_at_filter_path(tp));
                        let press = ev.event_type() == gdk::EventType::ButtonPress;
                        if press {
                            match row {
                                Some(r) => p.open_middle(r),
                                None => {
                                    crate::scroll::autoscroll(iv.upcast_ref(), ev);
                                }
                            }
                        }
                        glib::Propagation::Stop
                    }
                    b => p.nav_button(b),
                }
            });
        }
        {
            let w = w.clone();
            iv.connect_button_release_event(move |iv, ev| {
                // A press on one of several selected items without a drag:
                // now just it is selected (see icon_press).
                if let Some(p) = w.upgrade() {
                    if ev.button() == 1 {
                        p.band.borrow_mut().take();
                        let taken = p.press_keep.borrow_mut().take();
                        if let Some((keep, clicked)) = taken {
                            iv.unselect_all();
                            iv.select_path(&clicked);
                            *p.released_keep.borrow_mut() = Some((keep, clicked));
                        }
                    }
                }
                glib::Propagation::Proceed
            });
        }
        {
            let w = w.clone();
            iv.connect_popup_menu(move |_| {
                if let Some(p) = w.upgrade() {
                    let on_item = !p.selected_paths().is_empty();
                    p.emit(Event::Context { event: None, on_item });
                }
                true
            });
        }
    }

    /// The list view: selection, activation, clicks, context menu, column headers.
    fn connect_tree_view(self: &Rc<Self>) {
        let w = Rc::downgrade(self);
        let tv = &self.tree_view;
        {
            let w = w.clone();
            tv.selection().connect_changed(move |_| {
                if let Some(p) = w.upgrade() {
                    p.emit(Event::Selection);
                }
            });
        }
        {
            let block = self.tv_block.clone();
            tv.selection()
                .set_select_function(Some(Box::new(move |_, _, _, _| !block.get())));
        }
        {
            let w = w.clone();
            tv.connect_row_activated(move |_, _, _| {
                if let Some(p) = w.upgrade() {
                    p.emit(Event::Activate);
                }
            });
        }
        {
            let w = w.clone();
            tv.connect_button_press_event(move |tv, ev| {
                let Some(p) = w.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if ev.window() != tv.bin_window() {
                    return glib::Propagation::Proceed;
                }
                let (x, y) = ev.position();
                let path = tv
                    .path_at_pos(x as i32, y as i32)
                    .and_then(|(p, _, _, _)| p);
                let sel = tv.selection();
                match ev.button() {
                    3 => {
                        tv.grab_focus();
                        p.context_click(
                            ev,
                            path.as_ref(),
                            |tp| sel.path_is_selected(tp),
                            |tp| {
                                sel.unselect_all();
                                sel.select_path(tp);
                                tv.set_cursor(tp, None::<&gtk::TreeViewColumn>, false);
                            },
                            || sel.unselect_all(),
                        )
                    }
                    2 => {
                        let row = path.as_ref().and_then(|tp| p.row_at_filter_path(tp));
                        let press = ev.event_type() == gdk::EventType::ButtonPress;
                        // The rows fill the list's width, leaving hardly any
                        // empty space, so files and folders autoscroll too
                        // (when it's on and there's something to scroll);
                        // otherwise, or with Shift, a middle-click opens
                        // them, a folder in a new tab.
                        if press {
                            let shift = ev.state().contains(gdk::ModifierType::SHIFT_MASK);
                            match row {
                                Some(r) if shift || !crate::scroll::autoscroll(tv.upcast_ref(), ev) => p.open_middle(r),
                                Some(_) => {}
                                None => {
                                    crate::scroll::autoscroll(tv.upcast_ref(), ev);
                                }
                            }
                        }
                        glib::Propagation::Stop
                    }
                    1 => {
                        let mods = ev.state()
                            & (gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK);
                        match &path {
                            None => {
                                if mods.is_empty() {
                                    sel.unselect_all();
                                }
                            }
                            Some(tp) => {
                                if ev.event_type() == gdk::EventType::ButtonPress
                                    && mods.is_empty()
                                    && sel.path_is_selected(tp)
                                    && sel.count_selected_rows() > 1
                                {
                                    // Keep the multi-selection so it can be dragged.
                                    p.tv_block.set(true);
                                    *p.tv_pending.borrow_mut() = Some(tp.clone());
                                }
                            }
                        }
                        glib::Propagation::Proceed
                    }
                    b => p.nav_button(b),
                }
            });
        }
        {
            let w = w.clone();
            tv.connect_button_release_event(move |tv, _| {
                if let Some(p) = w.upgrade() {
                    if p.tv_block.get() {
                        p.tv_block.set(false);
                        let taken = p.tv_pending.borrow_mut().take();
                        if let Some(tp) = taken {
                            let sel = tv.selection();
                            sel.unselect_all();
                            sel.select_path(&tp);
                        }
                    }
                }
                glib::Propagation::Proceed
            });
        }
        {
            let w = w.clone();
            tv.connect_popup_menu(move |_| {
                if let Some(p) = w.upgrade() {
                    let on_item = !p.selected_paths().is_empty();
                    p.emit(Event::Context { event: None, on_item });
                }
                true
            });
        }
        // A right click on a column header: which columns to show (the
        // others' headers get it as they're added, see apply_column_choice).
        self.tree_view.connect_button_release_event(move |_, _| {
            HEADER_HELD.store(false, std::sync::atomic::Ordering::Relaxed);
            glib::Propagation::Proceed
        });
        self.wire_header(&self.name_col);
        // A column dragged to another place: kept in that order everywhere.
        {
            let w = w.clone();
            self.tree_view.connect_columns_changed(move |tv| {
                let Some(p) = w.upgrade() else { return };
                // A drag ends with its column put down.
                HEADER_HELD.store(false, std::sync::atomic::Ordering::Relaxed);
                // Not while columns are placed, or taken away (the pane closing).
                let all = tv.columns();
                if p.placing_columns.get() || p.columns_mode.get() || all.len() != p.added_cols.get() + 1 {
                    return;
                }
                // The name stays first (a column dropped before it goes after).
                if all.first() != Some(&p.name_col) {
                    p.placing_columns.set(true);
                    tv.move_column_after(&p.name_col, None::<&gtk::TreeViewColumn>);
                    p.placing_columns.set(false);
                }
                let chosen = config::with(|s| crate::details::known(&s.list_columns));
                let order: Vec<&'static str> = all
                    .iter()
                    .filter_map(|c| p.detail_cols.iter().find(|(_, dc)| dc == c).map(|(id, _)| *id))
                    .filter(|id| chosen.contains(id))
                    .collect();
                if order != chosen {
                    config::update(|s| s.list_columns = order.iter().map(|id| id.to_string()).collect());
                    // The other panes, once GTK is done with the drag.
                    let w = Rc::downgrade(&p);
                    glib::idle_add_local_once(move || {
                        if let Some(p) = w.upgrade() {
                            p.emit(Event::ColumnsChanged);
                        }
                    });
                }
            });
        }
        for (id, col) in &self.sort_cols {
            let w = w.clone();
            let id = *id;
            col.connect_clicked(move |_| {
                if let Some(p) = w.upgrade() {
                    let (cur, desc) = p.sort_state();
                    let name = crate::details::sort_name(id);
                    let desc = if cur == name { !desc } else { crate::details::get(name).is_some_and(|d| d.desc_first) };
                    p.emit(Event::Sort { by: name.to_string(), desc });
                }
            });
        }
    }

    /// Dragging items from and dropping onto both views.
    fn connect_dnd(self: &Rc<Self>) {
        let w = Rc::downgrade(self);
        enable_drag_source_iv(&self.icon_view);
        enable_drag_source_tv(&self.tree_view);
        let views: [gtk::Widget; 2] = [self.icon_view.clone().upcast(), self.tree_view.clone().upcast()];
        for view in views {
            // Files, and colours, patterns and emblems (see backgrounds.rs).
            let all: Vec<gtk::TargetEntry> = drop_targets().into_iter().chain(crate::backgrounds::targets()).collect();
            view.drag_dest_set(gtk::DestDefaults::empty(), &all, dnd_actions());
            // Files are moved by ops, which the monitor picks up. Never let GTK
            // remove the dragged row from the store behind our index's back.
            view.connect_drag_data_delete(|v, _| v.stop_signal_emission_by_name("drag-data-delete"));
            {
                let w = w.clone();
                view.connect_drag_begin(move |_, _| {
                    // GTK draws the drag icon from the items' cells next:
                    // they need their pictures (see mark_drawing), until the
                    // handler below.
                    icons::set_drawing(true);
                    if let Some(p) = w.upgrade() {
                        // The whole selection is dragged (see icon_press).
                        p.press_keep.borrow_mut().take();
                        set_drag_source_dir(Some(p.current().uri().to_string()));
                        let all_dirs = p.selected().iter().all(|r| r.is_dir);
                        DRAG_ALL_DIRS.with(|d| d.set(Some(all_dirs)));
                        p.tv_block.set(false);
                        p.tv_pending.borrow_mut().take();
                    }
                });
            }
            view.connect_local("drag-begin", true, |_| {
                icons::set_drawing(false);
                None
            });
            view.connect_drag_end(|_, _| {
                set_drag_source_dir(None);
            });
            {
                let w = w.clone();
                view.connect_drag_data_get(move |_, _, sel, _, _| {
                    if let Some(p) = w.upgrade() {
                        let uris = p.selected_uris();
                        let refs: Vec<&str> = uris.iter().map(|s| s.as_str()).collect();
                        sel.set_uris(&refs);
                    }
                });
            }
            {
                let w = w.clone();
                view.connect_drag_motion(move |view, ctx, x, y, time| {
                    let Some(p) = w.upgrade() else { return false };
                    // Nothing it can take (text, or out of an archive on Wayland): say so.
                    let Some(kind) = view.drag_dest_find_target(ctx, None) else {
                        p.highlight_drop(None);
                        ctx.drag_status(gdk::DragAction::empty(), time);
                        return true;
                    };
                    // An emblem goes on the item under it; a colour or
                    // pattern on the folder's background (not search results).
                    if crate::backgrounds::is_target(&kind.name()) {
                        let on = if kind.name() == crate::backgrounds::EMBLEM_TARGET { p.item_at(x, y) } else { None };
                        p.highlight_drop(on.as_ref().map(|t| &t.0));
                        let ok = if kind.name() == crate::backgrounds::EMBLEM_TARGET { on.is_some() } else { !p.is_searching() };
                        ctx.drag_status(if ok { gdk::DragAction::COPY } else { gdk::DragAction::empty() }, time);
                        return true;
                    }
                    let target = p.drop_target(x, y);
                    let from_self = ctx
                        .drag_get_source_widget()
                        .map(|s| &s == view)
                        .unwrap_or(false);
                    p.highlight_drop(target.as_ref().map(|t| &t.0));
                    let dest = match &target {
                        Some((_, uri)) => gio::File::for_uri(uri),
                        None => {
                            // Search results are no folder to drop into, as in Caja.
                            if from_self || p.is_searching() {
                                ctx.drag_status(gdk::DragAction::empty(), time);
                                return true;
                            }
                            p.current()
                        }
                    };
                    let a = choose_action(view, ctx, &dest);
                    ctx.drag_status(a, time);
                    true
                });
            }
            {
                let w = w.clone();
                view.connect_drag_leave(move |_, _, _| {
                    if let Some(p) = w.upgrade() {
                        p.highlight_drop(None);
                    }
                });
            }
            {
                let w = w.clone();
                view.connect_drag_drop(move |view, ctx, x, y, time| {
                    // Out of an archive, where the files go (not search results).
                    let dest = || {
                        w.upgrade().and_then(|p| match p.drop_target(x, y) {
                            Some((_, uri)) => Some(gio::File::for_uri(&uri)),
                            None => (!p.is_searching()).then(|| p.current()),
                        })
                    };
                    match view.drag_dest_find_target(ctx, None) {
                        // Out of an archive: the archive manager saves the files.
                        Some(t) if crate::xds::is(&t) => {
                            if !dest().is_some_and(|d| crate::xds::start(view, ctx, &d, time)) {
                                ctx.drag_finish(false, false, time);
                            }
                        }
                        // Out of Ark: Ark extracts them.
                        Some(t) if crate::arkdnd::is(&t) => match dest() {
                            Some(d) => crate::arkdnd::start(view, ctx, &d, time),
                            None => ctx.drag_finish(false, false, time),
                        },
                        Some(t) => view.drag_get_data(ctx, &t, time),
                        None => ctx.drag_finish(false, false, time),
                    }
                    true
                });
            }
            {
                let w = w.clone();
                view.connect_drag_data_received(move |view, ctx, x, y, sel, _, time| {
                    // The views' own handlers would insert rows into the model.
                    view.stop_signal_emission_by_name("drag-data-received");
                    if crate::xds::is(&sel.target()) {
                        crate::xds::finish(ctx, sel, time);
                        if let Some(p) = w.upgrade() {
                            p.highlight_drop(None);
                        }
                        return;
                    }
                    if crate::arkdnd::received(view, ctx, sel, time) {
                        if let Some(p) = w.upgrade() {
                            p.highlight_drop(None);
                        }
                        return;
                    }
                    let Some(p) = w.upgrade() else { return };
                    let kind = sel.target().name();
                    if crate::backgrounds::is_target(&kind) {
                        p.highlight_drop(None);
                        let d = crate::backgrounds::parse(&kind, &sel.data());
                        ctx.drag_finish(d.is_some(), false, time);
                        if let Some(d) = d {
                            p.background_dropped(x, y, d);
                        }
                        return;
                    }
                    let mut uris: Vec<String> = sel.uris().iter().map(|s| s.to_string()).collect();
                    let target = p.drop_target(x, y);
                    if target.is_none() && p.is_searching() {
                        uris.clear();
                    }
                    let dest = target.map(|(_, u)| gio::File::for_uri(&u)).unwrap_or_else(|| p.current());
                    let action = choose_action(view, ctx, &dest);
                    p.highlight_drop(None);
                    ctx.drag_finish(!uris.is_empty(), false, time);
                    if !uris.is_empty() {
                        p.emit(Event::Drop { uris, dest, action });
                    }
                });
            }
        }
    }

    /// Inline rename, and ← / → at a row's end in the icon views.
    fn connect_rename(self: &Rc<Self>) {
        let w = Rc::downgrade(self);
        for cell in [&self.icon_text, &self.list_text] {
            let in_icons = cell == &self.icon_text;
            {
                let w = w.clone();
                cell.connect_editing_started(move |_, editable, path| {
                    let row = w.upgrade().and_then(|p| p.row_at_filter_path(&path));
                    // Folders have no extension: select the whole name.
                    let is_dir = row.as_ref().map(|r| r.is_dir).unwrap_or(false);
                    // In the icon view the item's own name would show through
                    // the edit field: it's hidden now that the field has it.
                    if let (true, Some(p), Some(r)) = (in_icons, w.upgrade(), row.as_ref()) {
                        *p.edit_hidden.borrow_mut() = Some(r.uri.clone());
                        p.icon_view.queue_draw();
                    }
                    if let Some(entry) = editable.downcast_ref::<gtk::Entry>() {
                        // Some themes draw the field without a background.
                        entry.style_context().add_provider(&edit_field_css(), gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
                        let entry = entry.clone();
                        glib::idle_add_local_once(move || {
                            let text = entry.text().to_string();
                            let (stem, _) = if is_dir { (text.as_str(), "") } else { util::split_ext(&text) };
                            entry.select_region(0, stem.chars().count() as i32);
                        });
                    }
                });
            }
            {
                let w = w.clone();
                cell.connect_edited(move |cell, path, new| {
                    cell.set_editable(false);
                    let Some(p) = w.upgrade() else { return };
                    p.editing.borrow_mut().take();
                    p.show_edited_name();
                    if let Some(r) = p.row_at_filter_path(&path) {
                        if !new.is_empty() && new != r.name {
                            p.emit(Event::Rename { uri: r.uri, new_name: new.to_string() });
                        }
                    }
                });
            }
            {
                let ed = self.editing.clone();
                let w = w.clone();
                cell.connect_editing_canceled(move |cell| {
                    cell.set_editable(false);
                    ed.borrow_mut().take();
                    if let Some(p) = w.upgrade() {
                        p.show_edited_name();
                    }
                });
            }
        }

        // ← / → in the icon views go on to the next row at a row's end (and
        // back to the previous row's end at its start), instead of stopping.
        {
            let w = w.clone();
            self.icon_view.connect_key_press_event(move |_, ev| match w.upgrade() {
                Some(p) => {
                    use gdk::keys::constants as k;
                    let nav = [k::Up, k::Down, k::Left, k::Right, k::Home, k::End, k::Page_Up, k::Page_Down, k::KP_Up, k::KP_Down, k::KP_Left, k::KP_Right];
                    if nav.contains(&ev.keyval()) {
                        if !ev.state().contains(gdk::ModifierType::SHIFT_MASK) {
                            p.range_anchor.set(None);
                        }
                        p.keyboard_focus.set(true);
                        p.icon_view.queue_draw();
                    }
                    p.icon_arrow_key(ev)
                }
                None => glib::Propagation::Proceed,
            });
        }
    }

    /// Typeahead, the filter and zooming with Ctrl and the wheel.
    fn connect_typeahead(self: &Rc<Self>) {
        let w = Rc::downgrade(self);
        for view in [self.icon_view.clone().upcast::<gtk::Widget>(), self.tree_view.clone().upcast()] {
            let w = w.clone();
            view.connect_key_press_event(move |_, ev| {
                let Some(p) = w.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if p.enter_opens(ev) {
                    return glib::Propagation::Stop;
                }
                p.typeahead_key(ev)
            });
            // Clicking or leaving the view ends typing to select.
            let w3 = Rc::downgrade(self);
            view.connect_button_press_event(move |_, _| {
                if let Some(p) = w3.upgrade() {
                    p.end_typeahead();
                }
                glib::Propagation::Proceed
            });
            let w4 = Rc::downgrade(self);
            view.connect_focus_out_event(move |_, _| {
                if let Some(p) = w4.upgrade() {
                    p.end_typeahead();
                }
                glib::Propagation::Proceed
            });
            let w2 = Rc::downgrade(self);
            view.connect_scroll_event(move |_, ev| {
                // While typing to select, the wheel goes to the previous /
                // next match, as in Caja.
                if let Some(p) = w2.upgrade() {
                    if !p.typeahead.borrow().is_empty() {
                        p.typeahead_scroll(ev);
                        return glib::Propagation::Stop;
                    }
                }
                if ev.state().contains(gdk::ModifierType::CONTROL_MASK) {
                    if let Some(p) = w2.upgrade() {
                        let dy = crate::scroll::vertical_steps(ev);
                        if dy < 0.0 {
                            p.zoom(1);
                        } else if dy > 0.0 {
                            p.zoom(-1);
                        }
                    }
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
        }
    }
}
