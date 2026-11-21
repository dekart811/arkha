//! File / folder properties dialog (Basic, Emblems, Permissions, Open With,
//! Notes, Media Info), as Caja's. Emblems, notes and a custom icon are GVfs
//! metadata, as Caja keeps them, so the two share them.

use crate::i18n::{fill, n_, tr, trn};
use gio::prelude::*;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::metadata::Meta;
use crate::{config, icons, util};

fn row(grid: &gtk::Grid, r: i32, key: &str, val: &gtk::Widget) {
    let k = gtk::Label::new(Some(key));
    k.set_xalign(1.0);
    k.set_yalign(0.0);
    k.style_context().add_class("dim-label");
    grid.attach(&k, 0, r, 1, 1);
    val.set_hexpand(true);
    grid.attach(val, 1, r, 1, 1);
}

fn value_label(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_selectable(true);
    l.set_line_wrap(true);
    l.set_line_wrap_mode(pango::WrapMode::WordChar);
    l.set_max_width_chars(48);
    l
}

/// Total size and item count of a set of files, computed in a thread.
/// `count_top` counts the given items themselves (a multi-selection), otherwise
/// only what's inside them (a single folder's contents).
fn deep_count(uris: Vec<String>, count_top: bool, label: gtk::Label) {
    let (tx, rx) = async_channel::unbounded::<(u64, u64, bool)>();
    std::thread::spawn(move || {
        let mut bytes = 0u64;
        let mut items = 0u64;
        let mut stack: Vec<std::path::PathBuf> = vec![];
        for u in &uris {
            if let Some(p) = gio::File::for_uri(u).path() {
                stack.push(p);
            }
        }
        let top: std::collections::HashSet<_> = stack.iter().cloned().collect();
        let mut last = std::time::Instant::now();
        while let Some(p) = stack.pop() {
            let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
            if count_top || !top.contains(&p) {
                items += 1;
            }
            if md.is_dir() {
                if let Ok(rd) = std::fs::read_dir(&p) {
                    for e in rd.flatten() {
                        stack.push(e.path());
                    }
                }
            } else {
                bytes += md.len();
            }
            if last.elapsed().as_millis() > 150 {
                last = std::time::Instant::now();
                if tx.send_blocking((bytes, items, false)).is_err() {
                    return;
                }
            }
        }
        let _ = tx.send_blocking((bytes, items, true));
    });
    glib::MainContext::default().spawn_local(async move {
        while let Ok((b, n, done)) = rx.recv().await {
            let text = fill(&trn("{n} item, totalling {size}", "{n} items, totalling {size}", n), &[("n", &n), ("size", &util::format_size(b))]);
            // Still counting.
            label.set_text(&if done { text } else { format!("{text} …") });
        }
    });
}

pub fn show(parent: &gtk::Window, files: Vec<gio::File>) {
    if files.is_empty() {
        return;
    }
    let dialog = gtk::Dialog::with_buttons(None, Some(parent), gtk::DialogFlags::DESTROY_WITH_PARENT, &[(tr("_Close").as_str(), gtk::ResponseType::Close)]);
    // Wide enough for the Emblems page's columns, which fill in later (see fill).
    dialog.set_default_size(620, -1);
    dialog.connect_response(|d, _| d.close());
    // The tab last chosen (by a click or a key), kept for items dropped in.
    let chosen: Rc<std::cell::RefCell<Option<String>>> = Rc::default();
    // (Kept with the window for the note field, which passes files dropped
    // on it to the window: see file_drops_to_window.)
    unsafe { dialog.set_data(CHOSEN, chosen.clone()) };
    if !show_items(&dialog, files, &chosen) {
        util::error_dialog(Some(parent), &tr("Could not read properties"), &tr("The file may have been removed."));
        dialog.close();
        return;
    }
    // Items dragged into the window are shown in it instead, on the tab
    // chosen before if they have it (Media Info of one video, then of the
    // next): no window to close and open again for each.
    dialog.drag_dest_set(gtk::DestDefaults::ALL, &[gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::empty(), 0)], gdk::DragAction::COPY | gdk::DragAction::MOVE | gdk::DragAction::LINK);
    dialog.connect_drag_data_received(|d, _, _, _, data, _, _| show_dropped(d, data));
    dialog.show_all();
}

/// Where a Properties window keeps the tab chosen last (see show).
const CHOSEN: &str = "arkha-props-chosen";

/// Items dropped into the window (on it or its note field): shown in it.
fn show_dropped(dialog: &gtk::Dialog, data: &gtk::SelectionData) {
    let files: Vec<gio::File> = data.uris().iter().filter(|u| shows_properties(u)).map(|u| gio::File::for_uri(u)).collect();
    let Some(chosen) = (unsafe { dialog.data::<Rc<std::cell::RefCell<Option<String>>>>(CHOSEN) }).map(|c| unsafe { c.as_ref() }.clone()) else { return };
    if !files.is_empty() && !show_items(dialog, files, &chosen) {
        util::error_dialog(Some(dialog.upcast_ref()), &tr("Could not read properties"), &tr("The file may have been removed."));
    }
}

/// Whether a dropped link is for Properties: not a web page's (a link
/// dragged out of a web browser), which has none, and looked up as a file
/// would be a download while the window waits.
fn shows_properties(uri: &str) -> bool {
    let scheme = uri.split_once(':').map_or("", |(s, _)| s).to_ascii_lowercase();
    scheme != "http" && scheme != "https"
}

/// The note field passes files dropped on it to the window, as the rest of
/// the window takes them: it took the drop itself, wanting only text, and
/// nothing happened. (Its own drops can't just be turned off: it sets them
/// up again whenever its text changes.) Text dropped on it is its own.
fn file_drops_to_window(view: &gtk::TextView, dialog: &gtk::Dialog) {
    let files = |ctx: &gdk::DragContext| ctx.list_targets().iter().any(|t| t.name() == "text/uri-list");
    view.connect_drag_motion(move |_, ctx, _, _, time| {
        if !files(ctx) {
            return false;
        }
        ctx.drag_status(gdk::DragAction::COPY, time);
        true
    });
    view.connect_drag_drop(move |v, ctx, _, _, time| {
        if !files(ctx) {
            return false;
        }
        // GTK hands over only data the field's list of targets names: for
        // this drop, files too (the field makes the list anew as it likes).
        let uris = gdk::Atom::intern("text/uri-list");
        if let Some(list) = v.drag_dest_get_target_list() {
            list.add(&uris, 0, 0);
        }
        v.drag_get_data(ctx, &uris, time);
        true
    });
    let d = dialog.downgrade();
    view.connect_drag_data_received(move |v, ctx, _, _, data, _, time| {
        if data.target().name() != "text/uri-list" {
            return;
        }
        v.stop_signal_emission_by_name("drag-data-received");
        ctx.drag_finish(true, false, time);
        if let Some(d) = d.upgrade() {
            // Once the drop is over: the field goes with the tab.
            let data = data.clone();
            glib::idle_add_local_once(move || show_dropped(&d, &data));
        }
    });
}

/// Fills `dialog` with the properties of `files` (replacing what it
/// showed), on the tab `chosen` if they have it. False if none can be read
/// (vanished): then it shows what it did.
fn show_items(dialog: &gtk::Dialog, files: Vec<gio::File>, chosen: &Rc<std::cell::RefCell<Option<String>>>) -> bool {

    // Not the metadata (emblems, notes, a custom icon): metadata.rs reads
    // it, and the pages that show it are filled when it comes.
    let attrs = "standard::*,time::*,unix::*,owner::*,access::*,id::filesystem,filesystem::*,selinux::context";
    // Keep files and infos paired up, dropping files that vanished.
    let (files, infos): (Vec<gio::File>, Vec<gio::FileInfo>) = files
        .into_iter()
        .filter_map(|f| {
            let info = f.query_info(attrs, gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE).ok()?;
            Some((f, info))
        })
        .unzip();
    if infos.is_empty() {
        return false;
    }
    dialog.set_title(&if files.len() == 1 {
        fill(&tr("{name} Properties"), &[("name", &util::display_name(&files[0]))])
    } else {
        fill(&trn("Properties of {n} item", "Properties of {n} items", files.len() as u64), &[("n", &files.len())])
    });
    // What it showed goes: its tabs end what they had going (a note being
    // typed is saved, checksums being worked out stop).
    for old in dialog.content_area().children().into_iter().filter(|c| c.is::<gtk::Notebook>()) {
        unsafe { old.destroy() };
    }
    let nb = gtk::Notebook::new();
    nb.set_border_width(6);
    dialog.content_area().pack_start(&nb, true, true, 0);
    // Gone when other items are shown (see the metadata, below).
    let shown_still = nb.downgrade();

    let img = gtk::Image::new();
    if let Some(pb) = icons::icon_pixbuf(icons::item_icon(&infos[0], &[]).as_ref(), 64) {
        img.set_from_pixbuf(Some(&pb));
    }
    // The tabs hidden in Preferences → Properties aren't made at all
    // (Media Info runs mediainfo, Open With looks for the applications).
    let hidden = config::get().hidden_props_tabs;
    let shown = |id: &str| !hidden.iter().any(|h| h == id);
    // Each item's metadata, once read.
    let metas: Rc<std::cell::RefCell<Option<Vec<Meta>>>> = Rc::default();
    add_page(&nb, &basic_page(dialog, &files, &infos, &img, &metas), "basic", &tr("Basic"));
    // Filled once the metadata is read (below).
    let emblems_slot = reading_page();
    if shown("emblems") {
        add_page(&nb, &emblems_slot, "emblems", &tr("Emblems"));
    }
    let notes_slot = reading_page();
    if shown("permissions") && infos.iter().all(|i| i.has_attribute("unix::mode")) {
        add_page(&nb, &permissions_page(dialog, &files, &infos), "permissions", &tr("Permissions"));
    }
    if shown("open-with") && files.len() == 1 && infos[0].file_type() != gio::FileType::Directory {
        if let Some(ctype) = util::info_content_type(&infos[0]) {
            add_page(&nb, &open_with_page(dialog, &infos[0], &ctype), "open-with", &tr("Open With"));
        }
    }
    if shown("notes") && files.len() == 1 {
        add_page(&nb, &notes_slot, "notes", &tr("Notes"));
    }
    // The metadata read in the background; then the pages that show it.
    {
        let (d, files, img, info, metas) = (dialog.clone(), files.clone(), img, infos[0].clone(), metas);
        crate::metadata::read_items(files.iter().map(|f| f.uri().to_string()).collect(), move |found| {
            // Read for items no longer shown (others dropped in meanwhile):
            // their tabs are gone.
            if shown_still.upgrade().is_none_or(|nb| nb.parent().is_none()) {
                return;
            }
            let found: Vec<Meta> = found.into_iter().map(|(_, m)| m).collect();
            replace_contents(&emblems_slot, &emblems_page(&d, &files, found.iter().map(|m| m.emblems.clone()).collect()));
            replace_contents(&notes_slot, &notes_page(&d, &files[0], &found.first().map(|m| m.note.clone()).unwrap_or_default()));
            // (Filled after the rest: their fields too.)
            no_drops_in_fields(emblems_slot.upcast_ref());
            if let Some(pb) = found.first().and_then(|m| icons::icon_pixbuf(icons::item_icon(&m.applied_to(&info), &[]).as_ref(), 64)) {
                img.set_from_pixbuf(Some(&pb));
            }
            *metas.borrow_mut() = Some(found);
        });
    }

    // ----------------------------------------------------------- Media Info
    if shown("media") && files.len() == 1 && infos[0].file_type() == gio::FileType::Regular {
        let ctype = util::info_content_type(&infos[0]).unwrap_or_default();
        if let (Some(path), true) = (files[0].path(), media_info_wanted(&ctype)) {
            add_page(&nb, &media_info_page(path), "media", &tr("Media Info"));
        }
    }

    // ------------------------------------------------------------ Checksums
    if shown("checksums") && files.len() == 1 && infos[0].file_type() == gio::FileType::Regular {
        let page = checksums_page(&files[0], infos[0].size().max(0) as u64);
        add_page(&nb, &page, "checksums", &tr("Checksums"));
    }

    // Draggable into another order, which is kept, as in Rename All.
    util::order_tabs(&nb, config::get().props_tab_order, |order| config::update(|s| s.props_tab_order = order));
    hide_tabs_by_right_click(&nb);
    nb.show_all();
    // Its look worked out again: made in a window already shown (an item
    // dropped in), its pages kept the look of a book without tabs (no
    // page background, in Adwaita and others) when the tabs came.
    nb.reset_style();
    // The name field takes no drops: a file dropped on it went to it, which
    // takes only text, and nothing happened. Dropped there, it's now the
    // window's (typing and pasting are as before).
    no_drops_in_fields(nb.upcast_ref());
    // The tab chosen before, if these items have it (else Basic, the first,
    // and the choice is kept for the next ones that have it).
    if let Some(want) = chosen.borrow().clone() {
        if let Some(i) = (0..nb.n_pages()).find(|&i| nb.nth_page(Some(i)).is_some_and(|p| p.widget_name() == want)) {
            nb.set_current_page(Some(i));
        }
    }
    // From now on (adding the tabs selected the first one by itself).
    let chosen = chosen.clone();
    nb.connect_switch_page(move |_, page, _| {
        *chosen.borrow_mut() = Some(page.widget_name().to_string());
    });
    true
}

/// `w` and what's in it: text entries take no drops (see show_items; the
/// note field passes them on, see file_drops_to_window).
fn no_drops_in_fields(w: &gtk::Widget) {
    if w.is::<gtk::Entry>() {
        w.drag_dest_unset();
    }
    if let Some(c) = w.downcast_ref::<gtk::Container>() {
        for child in c.children() {
            no_drops_in_fields(&child);
        }
    }
}

/// The Basic tab: the icon (a click picks another), the name (editable for
/// one item), type, size or contents, location, free space, a drive's usage
/// and the dates; for several items their number, contents and location.
fn basic_page(dialog: &gtk::Dialog, files: &[gio::File], infos: &[gio::FileInfo], img: &gtk::Image, metas: &Rc<std::cell::RefCell<Option<Vec<Meta>>>>) -> gtk::Grid {
    let grid = gtk::Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(12);
    grid.set_border_width(12);
    let mut r = 0;

    let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    // A click on the icon picks another one, as in Caja.
    let icon_button = gtk::Button::new();
    icon_button.set_relief(gtk::ReliefStyle::None);
    icon_button.set_valign(gtk::Align::Center);
    icon_button.set_tooltip_text(Some(&tr("Choose another icon")));
    icon_button.add(img);
    {
        let (files, d, img, metas, info) = (files.to_vec(), dialog.clone(), img.clone(), metas.clone(), infos[0].clone());
        icon_button.connect_clicked(move |_| choose_icon(&d, &files, &img, &metas, &info));
    }
    head.pack_start(&icon_button, false, false, 0);

    if files.len() == 1 {
        let f = files[0].clone();
        let info = &infos[0];
        let name_entry = gtk::Entry::new();
        name_entry.set_text(&info.display_name());
        name_entry.set_hexpand(true);
        name_entry.set_valign(gtk::Align::Center);
        name_entry.set_editable(info.boolean("access::can-rename"));
        {
            let cur = std::rc::Rc::new(std::cell::RefCell::new(f.clone()));
            let d = dialog.clone();
            name_entry.connect_activate(move |e| {
                let new = e.text().to_string();
                let file = cur.borrow().clone();
                if new.is_empty() || Some(new.as_str()) == file.basename().map(|b| b.to_string_lossy().to_string()).as_deref() {
                    return;
                }
                match file.set_display_name(&new, gio::Cancellable::NONE) {
                    Ok(nf) => {
                        crate::ops::note_rename(&file, &nf);
                        *cur.borrow_mut() = nf;
                        d.set_title(&fill(&tr("{name} Properties"), &[("name", &new)]));
                    }
                    Err(err) => util::error_dialog(d.transient_for().as_ref(), &tr("Could not rename"), err.message()),
                }
            });
        }
        head.pack_start(&name_entry, true, true, 0);
        grid.attach(&head, 0, r, 2, 1);
        r += 1;

        let is_dir = info.file_type() == gio::FileType::Directory;
        let ctype = util::info_content_type(info).unwrap_or_default();
        let tdesc = if is_dir {
            tr("Folder (inode/directory)")
        } else {
            format!("{} ({ctype})", gio::content_type_get_description(&ctype))
        };
        row(&grid, r, &tr("Type:"), value_label(&tdesc).upcast_ref());
        r += 1;
        if util::info_is_symlink(info) {
            if let Some(t) = info.symlink_target() {
                row(&grid, r, &tr("Link target:"), value_label(&t.to_string_lossy()).upcast_ref());
                r += 1;
            }
        }
        let size_label = value_label("");
        if is_dir {
            size_label.set_text(&tr("Counting…"));
            if f.path().is_some() {
                deep_count(vec![f.uri().to_string()], false, size_label.clone());
            } else {
                size_label.set_text(&tr("Unknown"));
            }
            row(&grid, r, &tr("Contents:"), size_label.upcast_ref());
        } else {
            let s = info.size().max(0) as u64;
            size_label.set_text(&fill(&trn("{size} ({bytes} byte)", "{size} ({bytes} bytes)", s), &[("size", &util::format_size(s)), ("bytes", &group_digits(s))]));
            row(&grid, r, &tr("Size:"), size_label.upcast_ref());
        }
        r += 1;
        let parent_loc = f.parent().map(|p| util::pretty_location(&p)).unwrap_or_default();
        row(&grid, r, &tr("Location:"), value_label(&parent_loc).upcast_ref());
        r += 1;
        if let Ok(fsinfo) = f.query_filesystem_info("filesystem::free,filesystem::size", gio::Cancellable::NONE) {
            if fsinfo.has_attribute("filesystem::free") {
                row(
                    &grid,
                    r,
                    &tr("Free space:"),
                    value_label(&fill(
                        &tr("{free} of {total}"),
                        &[("free", &util::format_size(fsinfo.attribute_uint64("filesystem::free"))), ("total", &util::format_size(fsinfo.attribute_uint64("filesystem::size")))],
                    ))
                    .upcast_ref(),
                );
                r += 1;
            }
        }
        // A drive's top folder: how full it is, as in Caja.
        if is_dir && is_mount_root(&f) {
            if let Some(usage) = disk_usage(&f) {
                grid.attach(&gtk::Separator::new(gtk::Orientation::Horizontal), 0, r, 2, 1);
                r += 1;
                row(&grid, r, &tr("Disk usage:"), usage.upcast_ref());
                r += 1;
            }
        }
        grid.attach(&gtk::Separator::new(gtk::Orientation::Horizontal), 0, r, 2, 1);
        r += 1;
        for (key, attr) in [(tr("Accessed:"), "time::access"), (tr("Modified:"), "time::modified"), (tr("Created:"), "time::created")] {
            if info.has_attribute(attr) {
                row(&grid, r, &key, value_label(&util::format_time_long(info.attribute_uint64(attr) as i64)).upcast_ref());
                r += 1;
            }
        }
    } else {
        let l = value_label(&fill(&trn("{n} item", "{n} items", files.len() as u64), &[("n", &files.len())]));
        head.pack_start(&l, true, true, 0);
        grid.attach(&head, 0, r, 2, 1);
        r += 1;
        let size_label = value_label(&tr("Counting…"));
        let uris: Vec<String> = files.iter().map(|f| f.uri().to_string()).collect();
        deep_count(uris, true, size_label.clone());
        row(&grid, r, &tr("Contents:"), size_label.upcast_ref());
        r += 1;
        let parent_loc = files[0].parent().map(|p| util::pretty_location(&p)).unwrap_or_default();
        row(&grid, r, &tr("Location:"), value_label(&parent_loc).upcast_ref());
    }
    grid
}

/// The Permissions tab: owner and group, a check box per permission bit
/// (each item keeps its other bits), the special bits, the mode in octal,
/// Apply to Enclosed Items for folders, and the SELinux context.
fn permissions_page(dialog: &gtk::Dialog, files: &[gio::File], infos: &[gio::FileInfo]) -> gtk::Grid {
    let pg = gtk::Grid::new();
    pg.set_row_spacing(8);
    pg.set_column_spacing(12);
    pg.set_border_width(12);
    let mut r = 0;
    // Every file keeps its own mode; a checkbox only flips its one bit in each.
    let modes: Rc<Vec<Cell<u32>>> = Rc::new(infos.iter().map(|i| Cell::new(i.attribute_uint32("unix::mode"))).collect());
    let my_uid = unsafe { libc_getuid() };
    let can_change = infos.iter().all(|i| i.attribute_uint32("unix::uid") == my_uid || my_uid == 0);
    let native = files.iter().all(|f| f.path().is_some());
    // The owner and group, as Caja has them: the owner can only be
    // changed by root, the group by the owner, to one of their groups.
    let owner = owner_combo(dialog, files, infos, my_uid == 0 && native);
    let group = group_combo(dialog, files, infos, can_change && native, my_uid == 0);
    for (key, combo) in [(tr("Owner:"), &owner), (tr("Group:"), &group)] {
        let l = gtk::Label::new(Some(&key));
        l.set_xalign(1.0);
        l.style_context().add_class("dim-label");
        pg.attach(&l, 0, r, 1, 1);
        // Over the permission columns, not widening the first.
        pg.attach(combo, 1, r, 3, 1);
        r += 1;
    }
    let headers = [String::new(), tr("Read"), tr("Write"), tr("Execute")];
    for (c, h) in headers.iter().enumerate() {
        let l = gtk::Label::new(Some(h));
        l.style_context().add_class("dim-label");
        pg.attach(&l, c as i32, r, 1, 1);
    }
    r += 1;
    let files_rc = Rc::new(files.to_vec());
    let any_dir = infos.iter().any(|i| i.file_type() == gio::FileType::Directory);
    let octal = value_label("");
    octal.style_context().add_class("dim-label");
    let update_octal = {
        let modes = modes.clone();
        let octal = octal.clone();
        Rc::new(move || {
            let first = modes[0].get() & 0o7777;
            let text = if modes.iter().all(|m| m.get() & 0o7777 == first) {
                fill(&tr("Mode: {mode}"), &[("mode", &format!("{first:04o}"))])
            } else {
                tr("Mode: mixed")
            };
            // TRANSLATORS: after "Mode: 0755", for folders.
            octal.set_text(&if any_dir { format!("{text}  {}", tr("(execute = enter folder)")) } else { text });
        })
    };
    update_octal();
    // A check box for one permission bit, flipping it in every file.
    let bit_box = {
        let (modes, files, update_octal, d) = (modes.clone(), files_rc.clone(), update_octal, dialog.clone());
        move |bit: u32, label: Option<&str>| -> gtk::CheckButton {
            let cb = match label {
                Some(l) => gtk::CheckButton::with_label(l),
                None => gtk::CheckButton::new(),
            };
            let on = modes.iter().filter(|m| m.get() & bit != 0).count();
            cb.set_active(on == modes.len());
            cb.set_inconsistent(on != 0 && on != modes.len());
            cb.set_sensitive(can_change);
            let (modes, files, update_octal, d) = (modes.clone(), files.clone(), update_octal.clone(), d.clone());
            cb.connect_toggled(move |cb| {
                cb.set_inconsistent(false);
                let set = cb.is_active();
                let mut changed = vec![];
                for (f, m) in files.iter().zip(modes.iter()) {
                    let new = if set { m.get() | bit } else { m.get() & !bit };
                    if let Err(e) = f.set_attribute_uint32("unix::mode", new & 0o7777, gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) {
                        util::error_dialog(d.transient_for().as_ref(), &tr("Could not change permissions"), e.message());
                        break;
                    }
                    changed.push((f.clone(), m.get(), new));
                    m.set(new);
                }
                crate::ops::note_modes(changed);
                update_octal();
            });
            cb
        }
    };
    for (i, who) in [tr("Owner"), tr("Group"), tr("Others")].iter().enumerate() {
        let l = gtk::Label::new(Some(who));
        l.set_xalign(1.0);
        pg.attach(&l, 0, r, 1, 1);
        for j in 0..3 {
            let cb = bit_box(1u32 << (8 - (i * 3 + j)), None);
            cb.set_halign(gtk::Align::Center);
            pg.attach(&cb, j as i32 + 1, r, 1, 1);
        }
        r += 1;
    }
    // setuid, setgid, sticky: the latter for folders like /tmp, where
    // only an item's owner may delete or rename it.
    {
        let l = gtk::Label::new(Some(&tr("Special")));
        l.set_xalign(1.0);
        l.set_yalign(0.0);
        pg.attach(&l, 0, r, 1, 1);
        let specials = gtk::Box::new(gtk::Orientation::Vertical, 2);
        for (bit, label, tip) in [
            (0o4000, tr("Set user ID"), tr("Run as the file’s owner, whoever starts it")),
            (0o2000, tr("Set group ID"), tr("Run with the file’s group; in a folder, new items get the folder’s group")),
            (0o1000, tr("Sticky"), tr("In a folder: only an item’s owner can delete or rename it")),
        ] {
            let cb = bit_box(bit, Some(&label));
            cb.set_tooltip_text(Some(&tip));
            specials.add(&cb);
        }
        pg.attach(&specials, 1, r, 3, 1);
        r += 1;
    }
    pg.attach(&octal, 0, r, 4, 1);
    // Apply to what's inside, as Caja does: folders get this mode,
    // files its read and write bits (their own execute bits stay).
    if can_change && any_dir {
        r += 1;
        let apply = gtk::Button::with_label(&tr("Apply to Enclosed Items…"));
        apply.set_halign(gtk::Align::Start);
        apply.set_tooltip_text(Some(&tr("Give the folders inside these permissions and this group, and the files inside the same read and write permissions and group")));
        let done = value_label("");
        done.style_context().add_class("dim-label");
        let grid = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        grid.pack_start(&apply, false, false, 0);
        grid.pack_start(&done, true, true, 0);
        pg.attach(&grid, 0, r, 4, 1);
        let (modes, files, d) = (modes, files_rc, dialog.clone());
        apply.connect_clicked(move |b| {
            let dirs: Vec<(std::path::PathBuf, u32)> = files
                .iter()
                .zip(modes.iter())
                .filter_map(|(f, m)| Some((f.path()?, m.get() & 0o7777)))
                .filter(|(p, _)| p.is_dir())
                .collect();
            if dirs.is_empty() {
                return;
            }
            let ask = gtk::MessageDialog::new(
                Some(&d),
                gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
                gtk::MessageType::Question,
                gtk::ButtonsType::None,
                &tr("Apply these permissions to everything inside?"),
            );
            ask.set_secondary_text(Some(&if my_uid == 0 {
                tr("Every folder inside gets the same permissions, owner and group, every file the same read and write permissions, owner and group. This can’t be undone.")
            } else {
                tr("Every folder inside gets the same permissions and group, every file the same read and write permissions and group. This can’t be undone.")
            }));
            ask.add_button(&tr("_Cancel"), gtk::ResponseType::Cancel);
            ask.add_button(&tr("_Apply"), gtk::ResponseType::Accept);
            let ok = ask.run() == gtk::ResponseType::Accept;
            ask.close();
            if !ok {
                return;
            }
            b.set_sensitive(false);
            done.set_text(&tr("Applying…"));
            let (b, done) = (b.clone(), done.clone());
            glib::MainContext::default().spawn_local(async move {
                let (changed, failed) = gio::spawn_blocking(move || apply_enclosed(&dirs, my_uid == 0)).await.unwrap_or((0, 0));
                b.set_sensitive(true);
                let items = |n: usize| fill(&trn("{n} item", "{n} items", n as u64), &[("n", &n)]);
                done.set_text(&match failed {
                    0 => fill(&tr("Changed {items}."), &[("items", &items(changed))]),
                    // TRANSLATORS: {items} and {failed} are numbers of items: "12 items".
                    f => fill(&tr("Changed {items}; {failed} couldn’t be changed (not yours?)."), &[("items", &items(changed)), ("failed", &items(f))]),
                });
            });
        });
    }
    // The SELinux security context, where the system has one.
    if let Some(ctx) = infos[0].attribute_string("selinux::context").filter(|c| !c.is_empty() && files.len() == 1) {
        r += 1;
        row(&pg, r, &tr("Security context:"), value_label(&ctx).upcast_ref());
    }
    if !can_change {
        r += 1;
        let l = value_label(&tr("You are not the owner, so you can't change these permissions."));
        pg.attach(&l, 0, r, 4, 1);
    }
    pg
}

/// The Open With tab: the programs for `ctype`, with Set as Default.
fn open_with_page(dialog: &gtk::Dialog, info: &gio::FileInfo, ctype: &str) -> gtk::Box {
    let ob = gtk::Box::new(gtk::Orientation::Vertical, 8);
    ob.set_border_width(12);
    let l = value_label(&fill(
        &tr("Select an application to open “{name}” and other files of type “{type}”:"),
        &[("name", &info.display_name()), ("type", &gio::content_type_get_description(ctype))],
    ));
    ob.pack_start(&l, false, false, 0);
    let w = gtk::AppChooserWidget::new(ctype);
    w.set_show_default(true);
    w.set_show_recommended(true);
    w.set_show_fallback(true);
    w.set_vexpand(true);
    w.set_size_request(-1, 260);
    ob.pack_start(&w, true, true, 0);
    let set_btn = gtk::Button::with_mnemonic(&tr("_Set as Default"));
    set_btn.set_halign(gtk::Align::End);
    {
        let ctype = ctype.to_string();
        let d = dialog.clone();
        set_btn.connect_clicked(move |_| {
            if let Some(app) = w.app_info() {
                if let Err(e) = app.set_as_default_for_type(&ctype) {
                    util::error_dialog(d.transient_for().as_ref(), &tr("Could not set default application"), e.message());
                }
            }
        });
    }
    ob.pack_start(&set_btn, false, false, 0);
    ob
}

/// The Checksums tab: a row for each checksum chosen in Preferences, worked
/// out only when its Calculate button is pressed, one at a time (the other
/// buttons wait), with a progress bar, the time left and Cancel; Copy puts
/// the result on the clipboard. Calculate All works out all those without a
/// result (all again if each has one) in a single read of the file.
fn checksums_page(file: &gio::File, size: u64) -> gtk::Box {
    use crate::checksums;
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.set_border_width(12);
    let algos = config::with(|s| checksums::offered(&s.checksum_order, &s.hidden_checksums));
    if algos.is_empty() {
        page.add(&value_label(&tr("No checksums are chosen: pick them in Preferences → Checksums.")));
        return page;
    }
    let hint = value_label(&tr("A checksum shows whether a file is exactly the one it should be: compare it with the one its source gives. Each is worked out from the whole file when you ask for it."));
    hint.style_context().add_class("dim-label");
    page.add(&hint);
    let grid = gtk::Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(12);
    page.add(&grid);
    // The calculation running, if any.
    let running: Rc<std::cell::RefCell<Option<gio::Cancellable>>> = Rc::default();
    let calc_buttons: Rc<std::cell::RefCell<Vec<gtk::Button>>> = Rc::default();
    // Each checksum's row: (checksum, its result, its Copy button).
    let rows: Rc<std::cell::RefCell<Vec<Target>>> = Rc::default();
    let bar = gtk::ProgressBar::new();
    bar.set_show_text(true);
    bar.set_hexpand(true);
    let cancel = gtk::Button::with_mnemonic(&tr("_Cancel"));
    let progress_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    progress_row.pack_start(&bar, true, true, 0);
    progress_row.pack_start(&cancel, false, false, 0);
    progress_row.set_no_show_all(true);
    {
        let running = running.clone();
        cancel.connect_clicked(move |_| {
            if let Some(c) = running.borrow().as_ref() {
                c.cancel();
            }
        });
    }
    // Closing the window, or showing other items in it, stops what's running.
    {
        let running = running.clone();
        page.connect_destroy(move |_| {
            if let Some(c) = running.borrow().as_ref() {
                c.cancel();
            }
        });
    }
    for (row, algo) in algos.into_iter().enumerate() {
        let row = row as i32;
        let name = gtk::Label::new(Some(algo.name()));
        name.set_xalign(0.0);
        name.set_valign(gtk::Align::Start);
        let value = gtk::Label::new(None);
        value.set_xalign(0.0);
        value.set_selectable(true);
        value.set_line_wrap(true);
        value.set_line_wrap_mode(pango::WrapMode::Char);
        value.set_hexpand(true);
        value.set_width_chars(32);
        value.set_max_width_chars(64);
        let attrs = pango::AttrList::new();
        attrs.insert(pango::AttrString::new_family("monospace"));
        value.set_attributes(Some(&attrs));
        let calc = gtk::Button::with_label(&tr("Calculate"));
        calc.set_valign(gtk::Align::Start);
        let copy = gtk::Button::from_icon_name(Some("edit-copy-symbolic"), gtk::IconSize::Button);
        copy.set_tooltip_text(Some(&tr("Copy it to the clipboard")));
        copy.set_valign(gtk::Align::Start);
        copy.set_sensitive(false);
        if let Some(a) = copy.accessible() {
            // TRANSLATORS: a button's name for screen readers: "Copy SHA-256".
            gtk::atk::prelude::AtkObjectExt::set_name(&a, &fill(&tr("Copy {checksum}"), &[("checksum", &algo.name())]));
        }
        {
            let value = value.clone();
            copy.connect_clicked(move |_| gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_text(&value.text()));
        }
        {
            let (running, buttons, bar, progress_row) = (running.clone(), calc_buttons.clone(), bar.clone(), progress_row.clone());
            let target = (algo, value.clone(), copy.clone());
            let uri = file.uri().to_string();
            calc.connect_clicked(move |_| {
                start_checksums(vec![target.clone()], &uri, size, &running, &buttons, &bar, &progress_row);
            });
        }
        calc_buttons.borrow_mut().push(calc.clone());
        rows.borrow_mut().push((algo, value.clone(), copy.clone()));
        grid.attach(&name, 0, row, 1, 1);
        grid.attach(&value, 1, row, 1, 1);
        grid.attach(&calc, 2, row, 1, 1);
        grid.attach(&copy, 3, row, 1, 1);
    }
    let all = gtk::Button::with_mnemonic(&tr("Calculate _All"));
    all.set_tooltip_text(Some(&tr("Work out every checksum above that has no result yet (all of them again if each has one), reading the file only once")));
    all.set_halign(gtk::Align::End);
    {
        let (running, buttons, bar, progress_row, rows) = (running, calc_buttons.clone(), bar, progress_row.clone(), rows);
        let uri = file.uri().to_string();
        all.connect_clicked(move |_| {
            let rows = rows.borrow();
            // A result: its Copy button is on (an error leaves it off).
            let missing: Vec<Target> = rows.iter().filter(|(_, _, copy)| !copy.is_sensitive()).cloned().collect();
            let targets = if missing.is_empty() { rows.clone() } else { missing };
            start_checksums(targets, &uri, size, &running, &buttons, &bar, &progress_row);
        });
    }
    calc_buttons.borrow_mut().push(all.clone());
    page.add(&all);
    page.add(&progress_row);
    page
}

/// A checksum to work out: the checksum, where its result goes, its Copy
/// button.
type Target = (crate::checksums::Algo, gtk::Label, gtk::Button);

/// Works out the checksums of `targets` for the file `uri` on a thread of
/// its own, in one read, showing the progress (bytes, time left) in `bar`,
/// every Calculate button waiting meanwhile; each result goes to its row.
fn start_checksums(
    targets: Vec<Target>,
    uri: &str,
    size: u64,
    running: &Rc<std::cell::RefCell<Option<gio::Cancellable>>>,
    buttons: &Rc<std::cell::RefCell<Vec<gtk::Button>>>,
    bar: &gtk::ProgressBar,
    progress_row: &gtk::Box,
) {
    if running.borrow().is_some() || targets.is_empty() {
        return;
    }
    let cancel = gio::Cancellable::new();
    *running.borrow_mut() = Some(cancel.clone());
    for b in buttons.borrow().iter() {
        b.set_sensitive(false);
    }
    let what = targets.iter().map(|(a, _, _)| a.name()).collect::<Vec<_>>().join(", ");
    bar.set_fraction(0.0);
    bar.set_text(Some(&fill(&tr("{checksums}: starting…"), &[("checksums", &what)])));
    progress_row.set_no_show_all(false);
    progress_row.show_all();
    enum Step {
        Read(u64),
        Done(Result<Vec<String>, glib::Error>),
    }
    let (tx, rx) = async_channel::unbounded::<Step>();
    let (uri, algos) = (uri.to_string(), targets.iter().map(|(a, _, _)| *a).collect::<Vec<_>>());
    std::thread::spawn(move || {
        let mut last = std::time::Instant::now();
        let r = crate::checksums::compute_many(&gio::File::for_uri(&uri), &algos, false, &cancel, &mut |read| {
            if last.elapsed() > std::time::Duration::from_millis(100) {
                last = std::time::Instant::now();
                let _ = tx.send_blocking(Step::Read(read));
            }
        });
        let _ = tx.send_blocking(Step::Done(r));
    });
    let started = std::time::Instant::now();
    let (running, buttons, bar, progress_row) = (running.clone(), buttons.clone(), bar.clone(), progress_row.clone());
    glib::MainContext::default().spawn_local(async move {
        while let Ok(step) = rx.recv().await {
            match step {
                Step::Read(read) => {
                    if size > 0 {
                        bar.set_fraction((read as f64 / size as f64).clamp(0.0, 1.0));
                    }
                    // TRANSLATORS: the checksums being worked out, then how far: "SHA-256: 1.2 GB of 4 GB, 10 s left".
                    bar.set_text(Some(&fill(&tr("{checksums}: {progress}"), &[("checksums", &what), ("progress", &crate::ops::bytes_detail(read, size, started.elapsed()))])));
                }
                Step::Done(r) => {
                    match r {
                        Ok(sums) => {
                            for ((_, value, copy), sum) in targets.iter().zip(sums) {
                                value.set_text(&sum);
                                copy.set_sensitive(true);
                            }
                        }
                        Err(e) if e.matches(gio::IOErrorEnum::Cancelled) => {}
                        Err(e) => {
                            for (_, value, _) in &targets {
                                value.set_text(&fill(&tr("Couldn’t read the file: {error}"), &[("error", &e.message())]));
                            }
                        }
                    }
                    running.borrow_mut().take();
                    for b in buttons.borrow().iter() {
                        b.set_sensitive(true);
                    }
                    progress_row.hide();
                    break;
                }
            }
        }
    });
}

/// A tab of Properties, called `id` in the order kept (see util::order_tabs).
/// A right-click on a tab (but Basic's) offers Hide This Tab: it goes at
/// once and stays hidden, as unticked in Preferences → Properties, where
/// it's shown again.
fn hide_tabs_by_right_click(nb: &gtk::Notebook) {
    nb.connect_button_press_event(|nb, ev| {
        if ev.button() != 3 || ev.event_type() != gdk::EventType::ButtonPress {
            return glib::Propagation::Proceed;
        }
        // The tab under the pointer: its label's area, a little larger
        // (the tab's padding), on the screen.
        let (rx, ry) = ev.root();
        let (rx, ry) = (rx as i32, ry as i32);
        let hit = (0..nb.n_pages()).filter_map(|i| nb.nth_page(Some(i))).find(|page| {
            let Some(label) = nb.tab_label(page) else { return false };
            let Some(win) = label.window() else { return false };
            let (_, ox, oy) = win.origin();
            let a = label.allocation();
            let (x, y) = (ox + a.x() - 8, oy + a.y() - 6);
            rx >= x && rx < x + a.width() + 16 && ry >= y && ry < y + a.height() + 12
        });
        let Some(page) = hit else { return glib::Propagation::Proceed };
        let id = page.widget_name().to_string();
        if id == "basic" {
            return glib::Propagation::Stop;
        }
        let menu = gtk::Menu::new();
        let item = gtk::MenuItem::with_mnemonic(&tr("_Hide This Tab"));
        {
            let nb = nb.clone();
            item.connect_activate(move |_| {
                config::update(|s| {
                    if !s.hidden_props_tabs.contains(&id) {
                        s.hidden_props_tabs.push(id.clone());
                    }
                });
                if let Some(n) = nb.page_num(&page) {
                    nb.remove_page(Some(n));
                }
            });
        }
        menu.append(&item);
        menu.show_all();
        menu.set_attach_widget(Some(nb));
        menu.popup_at_pointer(Some(ev));
        glib::Propagation::Stop
    });
}

/// A tab's title by its id (config::PROPS_TABS), for Preferences' list.
pub fn tab_title(id: &str) -> String {
    match id {
        "basic" => tr("Basic"),
        "emblems" => tr("Emblems"),
        "permissions" => tr("Permissions"),
        "open-with" => tr("Open With"),
        "checksums" => tr("Checksums"),
        "notes" => tr("Notes"),
        "media" => tr("Media Info"),
        other => other.to_string(),
    }
}

fn add_page(nb: &gtk::Notebook, page: &impl IsA<gtk::Widget>, id: &str, title: &str) {
    page.set_widget_name(id);
    nb.append_page(page, Some(&gtk::Label::new(Some(title))));
}

/// Whether mediainfo is installed.
pub fn mediainfo_available() -> bool {
    glib::find_program_in_path("mediainfo").is_some()
}

/// Whether the Media Info tab is wanted for a file of this content type.
fn media_info_wanted(ctype: &str) -> bool {
    let s = config::get();
    let kind_on = match ctype.split('/').next() {
        Some("video") => s.media_info_video,
        Some("audio") => s.media_info_audio,
        Some("image") => s.media_info_image,
        _ => s.media_info_other,
    };
    s.media_info && kind_on && mediainfo_available()
}

/// mediainfo's report for `path`, filled in once it has run in the background.
fn media_info_page(path: std::path::PathBuf) -> gtk::ScrolledWindow {
    let bx = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bx.set_border_width(12);
    let wait = value_label(&tr("Reading media information…"));
    wait.style_context().add_class("dim-label");
    bx.add(&wait);
    let sw = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    sw.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    sw.set_min_content_height(320);
    sw.add(&bx);
    glib::MainContext::default().spawn_local(async move {
        let argv = [std::ffi::OsStr::new("mediainfo"), path.as_os_str()];
        let flags = gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE;
        let result = match gio::Subprocess::newv(&argv, flags) {
            Ok(p) => match p.communicate_utf8_future(None).await {
                Ok((out, err)) => {
                    let out = out.map(|s| s.to_string()).unwrap_or_default();
                    let err = err.map(|s| s.trim().to_string()).unwrap_or_default();
                    let sections = parse_media_info(&out);
                    if !sections.is_empty() {
                        Ok(sections)
                    } else if !err.is_empty() {
                        Err(fill(&tr("mediainfo reported an error:\n{error}"), &[("error", &err)]))
                    } else {
                        Err(tr("mediainfo found no media information in this file."))
                    }
                }
                Err(e) => Err(fill(&tr("Could not read mediainfo’s output: {error}"), &[("error", &e.message())])),
            },
            Err(e) => Err(fill(&tr("Could not run mediainfo: {error}"), &[("error", &e.message())])),
        };
        bx.remove(&wait);
        match result {
            Ok(sections) => {
                let grid = gtk::Grid::new();
                grid.set_row_spacing(6);
                grid.set_column_spacing(12);
                let mut r = 0;
                for (title, fields) in sections {
                    let h = gtk::Label::new(None);
                    h.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(&title)));
                    h.set_xalign(0.0);
                    if r > 0 {
                        h.set_margin_top(10);
                    }
                    grid.attach(&h, 0, r, 2, 1);
                    r += 1;
                    for (k, v) in fields {
                        row(&grid, r, &k, value_label(&v).upcast_ref());
                        r += 1;
                    }
                }
                bx.add(&grid);
            }
            Err(msg) => bx.add(&value_label(&msg)),
        }
        bx.show_all();
    });
    sw
}

/// Splits mediainfo's text report into sections ("General", "Video", …) of
/// (field, value) pairs. Fields are "Name   : value" lines; a line without a
/// " : " starts a new section.
fn parse_media_info(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut sections: Vec<(String, Vec<(String, String)>)> = vec![];
    for line in text.lines().map(str::trim_end).filter(|l| !l.is_empty()) {
        match line.split_once(" : ") {
            Some((k, v)) if !sections.is_empty() => {
                sections.last_mut().unwrap().1.push((k.trim().to_string(), v.trim().to_string()));
            }
            Some(_) => {}
            None => sections.push((line.trim().to_string(), vec![])),
        }
    }
    sections.retain(|(_, fields)| !fields.is_empty());
    sections
}

/// Whether `f` is the top folder of a file system: `/`, a folder on another
/// device than its parent, or the root of a mount (a phone, a share).
fn is_mount_root(f: &gio::File) -> bool {
    use std::os::unix::fs::MetadataExt;
    if let Some(p) = f.path() {
        let Some(parent) = p.parent() else { return true };
        if let (Ok(a), Ok(b)) = (std::fs::metadata(&p), std::fs::metadata(parent)) {
            if a.dev() != b.dev() {
                return true;
            }
        }
    }
    f.find_enclosing_mount(gio::Cancellable::NONE).is_ok_and(|m| m.root().equal(f))
}

/// A pie of the used and free space on `f`'s file system, with the
/// numbers beside it.
fn disk_usage(f: &gio::File) -> Option<gtk::Widget> {
    let fs = f.query_filesystem_info("filesystem::size,filesystem::free,filesystem::used,filesystem::type", gio::Cancellable::NONE).ok()?;
    let total = fs.attribute_uint64("filesystem::size");
    if total == 0 {
        return None;
    }
    let free = fs.attribute_uint64("filesystem::free").min(total);
    let used = if fs.has_attribute("filesystem::used") { fs.attribute_uint64("filesystem::used") } else { total - free };
    let fraction = used as f64 / (used + free).max(1) as f64;
    let hb = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    let pie = gtk::DrawingArea::new();
    pie.set_size_request(72, 72);
    pie.set_valign(gtk::Align::Start);
    // The theme's selection color for what's used, a pale one for the rest.
    let colors = |w: &gtk::DrawingArea| {
        let sc = w.style_context();
        let used = sc.lookup_color("theme_selected_bg_color").unwrap_or(gdk::RGBA::new(0.21, 0.52, 0.89, 1.0));
        let fg = sc.lookup_color("theme_fg_color").unwrap_or(gdk::RGBA::new(0.5, 0.5, 0.5, 1.0));
        (used, gdk::RGBA::new(fg.red(), fg.green(), fg.blue(), 0.15))
    };
    pie.connect_draw(move |w, cr| {
        let (used, free) = colors(w);
        let (wd, ht) = (w.allocated_width() as f64, w.allocated_height() as f64);
        let (cx, cy, rad) = (wd / 2.0, ht / 2.0, wd.min(ht) / 2.0 - 1.0);
        let start = -std::f64::consts::FRAC_PI_2;
        let end = start + fraction * 2.0 * std::f64::consts::PI;
        cr.set_source_rgba(free.red(), free.green(), free.blue(), free.alpha());
        cr.arc(cx, cy, rad, 0.0, 2.0 * std::f64::consts::PI);
        let _ = cr.fill();
        if fraction > 0.0 {
            cr.set_source_rgba(used.red(), used.green(), used.blue(), used.alpha());
            cr.move_to(cx, cy);
            cr.arc(cx, cy, rad, start, end);
            cr.close_path();
            let _ = cr.fill();
        }
        glib::Propagation::Stop
    });
    hb.pack_start(&pie, false, false, 0);
    let legend = gtk::Grid::new();
    legend.set_row_spacing(4);
    legend.set_column_spacing(8);
    let swatch = |used_color: bool| {
        let sw = gtk::DrawingArea::new();
        sw.set_size_request(12, 12);
        sw.set_valign(gtk::Align::Center);
        sw.connect_draw(move |w, cr| {
            let (used, free) = colors(w);
            let c = if used_color { used } else { free };
            cr.set_source_rgba(c.red(), c.green(), c.blue(), c.alpha());
            cr.rectangle(0.0, 0.0, w.allocated_width() as f64, w.allocated_height() as f64);
            let _ = cr.fill();
            glib::Propagation::Stop
        });
        sw
    };
    let line = |r: i32, sw: Option<gtk::DrawingArea>, key: &str, val: String| {
        if let Some(sw) = sw {
            legend.attach(&sw, 0, r, 1, 1);
        }
        let k = gtk::Label::new(Some(key));
        k.set_xalign(0.0);
        k.style_context().add_class("dim-label");
        legend.attach(&k, 1, r, 1, 1);
        let v = gtk::Label::new(Some(&val));
        v.set_xalign(0.0);
        v.set_selectable(true);
        legend.attach(&v, 2, r, 1, 1);
    };
    // TRANSLATORS: a drive's used space and its share: "12 GB (40 %)".
    line(0, Some(swatch(true)), &tr("Used:"), fill(&tr("{size} ({percent} %)"), &[("size", &util::format_size(used)), ("percent", &format!("{:.0}", fraction * 100.0))]));
    line(1, Some(swatch(false)), &tr("Free:"), util::format_size(free));
    line(2, None, &tr("Total:"), util::format_size(total));
    if let Some(t) = fs.attribute_string("filesystem::type").filter(|t| !t.is_empty()) {
        line(3, None, &tr("File system:"), t.to_string());
    }
    hb.pack_start(&legend, false, false, 0);
    Some(hb.upcast())
}

/// Applies each folder's mode to everything inside it: subfolders get it
/// whole, files its read and write bits beside their own execute bits.
/// Its group too, and (`owner`: as root) its owner. Links are left alone.
/// (changed, failed).
fn apply_enclosed(dirs: &[(std::path::PathBuf, u32)], owner: bool) -> (usize, usize) {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let (mut changed, mut failed) = (0, 0);
    for (dir, mode) in dirs {
        let Ok(top) = std::fs::metadata(dir) else {
            failed += 1;
            continue;
        };
        let (uid, gid) = (top.uid(), top.gid());
        let mut stack = vec![dir.clone()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                failed += 1;
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
                if md.file_type().is_symlink() {
                    continue;
                }
                // First the owner and group: changing them can clear set-ID bits.
                let new_uid = Some(uid).filter(|u| owner && *u != md.uid());
                let new_gid = Some(gid).filter(|g| *g != md.gid());
                let mut did = false;
                if new_uid.is_some() || new_gid.is_some() {
                    match std::os::unix::fs::chown(&p, new_uid, new_gid) {
                        Ok(()) => did = true,
                        Err(_) => failed += 1,
                    }
                }
                let old = std::fs::symlink_metadata(&p).map_or(md.permissions().mode(), |m| m.permissions().mode()) & 0o7777;
                let new = if md.is_dir() {
                    stack.push(p.clone());
                    *mode
                } else {
                    (old & 0o7111) | (*mode & 0o666)
                };
                if new != old {
                    match std::fs::set_permissions(&p, std::fs::Permissions::from_mode(new)) {
                        Ok(()) => did = true,
                        Err(_) => failed += 1,
                    }
                }
                if did {
                    changed += 1;
                }
            }
        }
    }
    (changed, failed)
}

/// A page saying its contents are being read (see fill).
fn reading_page() -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let l = value_label(&tr("Reading…"));
    l.set_margin_start(12);
    l.set_margin_top(12);
    b.pack_start(&l, false, false, 0);
    b
}

/// A page's contents, in place of "Reading…".
fn replace_contents(page: &gtk::Box, content: &gtk::Widget) {
    for c in page.children() {
        page.remove(&c);
    }
    page.pack_start(content, true, true, 0);
    page.show_all();
}

/// Error messages from writing metadata, if any, shown over the window.
fn report(dialog: &gtk::Dialog, title: String) -> impl FnOnce(Vec<String>) + 'static {
    let parent = dialog.transient_for();
    move |errors| {
        if !errors.is_empty() {
            util::error_dialog(parent.as_ref(), &title, &errors.join("\n"));
        }
    }
}

/// The icon at the top of the Basic page, clicked: a picture for the items'
/// icon, or (Revert) their own again. Kept as metadata::custom-icon, as Caja does.
fn choose_icon(dialog: &gtk::Dialog, files: &[gio::File], img: &gtk::Image, metas: &Rc<std::cell::RefCell<Option<Vec<Meta>>>>, info: &gio::FileInfo) {
    let has_custom = metas.borrow().as_ref().is_some_and(|m| m.iter().any(|m| !m.icon.is_empty() || !m.icon_name.is_empty()));
    let chooser = gtk::FileChooserDialog::new(Some(&tr("Select Custom Icon")), Some(dialog), gtk::FileChooserAction::Open);
    if has_custom {
        chooser.add_button(&tr("_Revert"), gtk::ResponseType::Other(1));
    }
    chooser.add_button(&tr("_Cancel"), gtk::ResponseType::Cancel);
    chooser.add_button(&tr("_Open"), gtk::ResponseType::Accept);
    chooser.set_default_response(gtk::ResponseType::Accept);
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&tr("Images")));
    filter.add_pixbuf_formats();
    chooser.add_filter(filter);
    let pictures = glib::user_special_dir(glib::UserDirectory::Pictures).filter(|p| p.is_dir()).unwrap_or_else(glib::home_dir);
    let _ = chooser.set_current_folder(pictures);
    // A preview of the picture under the cursor.
    let preview = gtk::Image::new();
    chooser.set_preview_widget(&preview);
    chooser.connect_update_preview(move |c| {
        let pb = c.preview_filename().and_then(|p| icons::load_at_scale(&p, 128).ok());
        preview.set_from_pixbuf(pb.as_ref());
        c.set_preview_widget_active(pb.is_some());
    });
    let response = chooser.run();
    let picked = chooser.uri();
    chooser.close();
    use crate::metadata::Value;
    let icon = match response {
        gtk::ResponseType::Accept => match picked {
            Some(uri) => uri.to_string(),
            None => return,
        },
        gtk::ResponseType::Other(1) => String::new(),
        _ => return,
    };
    let value = if icon.is_empty() { Value::Unset } else { Value::Text(icon.clone()) };
    let writes = files
        .iter()
        .flat_map(|f| [(f.uri().to_string(), "custom-icon-name".to_string(), Value::Unset), (f.uri().to_string(), "custom-icon".to_string(), value.clone())])
        .collect();
    crate::metadata::write(writes, report(dialog, tr("Could not change the icon")));
    // Shown here at once; the views read it again once it's written.
    if let Some(ms) = metas.borrow_mut().as_mut() {
        for m in ms.iter_mut() {
            m.icon = icon.clone();
            m.icon_name.clear();
        }
    }
    let m = metas.borrow().as_ref().and_then(|m| m.first().cloned()).unwrap_or_default();
    if let Some(pb) = icons::icon_pixbuf(icons::item_icon(&m.applied_to(info), &[]).as_ref(), 64) {
        img.set_from_pixbuf(Some(&pb));
    }
    crate::window::refresh_everywhere(files);
}

/// A combo box of names by number (users or groups), the items' own
/// selected if they share it; picking one calls `set` for each file.
/// `failed`: the message's title if that doesn't work (shown translated).
fn id_combo(dialog: &gtk::Dialog, files: &[gio::File], choices: &[(u32, String)], current: Option<u32>, usable: bool, failed: &'static str, set: fn(&std::path::Path, u32) -> std::io::Result<()>) -> gtk::ComboBoxText {
    let combo = gtk::ComboBoxText::new();
    for (id, name) in choices {
        combo.append(Some(&id.to_string()), name);
    }
    if let Some(c) = current {
        combo.set_active_id(Some(&c.to_string()));
    }
    combo.set_sensitive(usable);
    let (files, d) = (files.to_vec(), dialog.clone());
    combo.connect_changed(move |c| {
        use std::os::unix::fs::MetadataExt;
        let Some(id) = c.active_id().and_then(|i| i.parse::<u32>().ok()) else { return };
        let ids = |p: &std::path::Path| std::fs::metadata(p).map(|m| (m.uid(), m.gid())).ok();
        let mut changed = vec![];
        for f in &files {
            let Some(p) = f.path() else { continue };
            let before = ids(&p);
            if let Err(e) = set(&p, id) {
                util::error_dialog(d.transient_for().as_ref(), &tr(failed), &format!("{}: {e}", util::display_name(f)));
                break;
            }
            if let (Some(b), Some(a)) = (before, ids(&p)) {
                changed.push((f.clone(), b, a));
            }
        }
        crate::ops::note_chowns(changed);
        crate::window::refresh_everywhere(&files);
    });
    combo
}

/// The items' owner, changeable only by root (as in Caja).
fn owner_combo(dialog: &gtk::Dialog, files: &[gio::File], infos: &[gio::FileInfo], usable: bool) -> gtk::ComboBoxText {
    let uids: Vec<u32> = infos.iter().map(|i| i.attribute_uint32("unix::uid")).collect();
    let mut users = all_users();
    for (uid, info) in uids.iter().zip(infos) {
        if !users.iter().any(|(u, _)| u == uid) {
            users.push((*uid, info.attribute_string("owner::user").map_or_else(|| uid.to_string(), |n| n.to_string())));
        }
    }
    let current = Some(uids[0]).filter(|u| uids.iter().all(|x| x == u));
    let combo = id_combo(dialog, files, &users, current, usable, n_("Could not change the owner"), |p, uid| std::os::unix::fs::chown(p, Some(uid), None));
    if !usable {
        combo.set_tooltip_text(Some(&tr("Only the administrator (root) can give an item to someone else")));
    }
    combo
}

/// The items' group: the owner can choose one of their own groups; root
/// any group.
fn group_combo(dialog: &gtk::Dialog, files: &[gio::File], infos: &[gio::FileInfo], usable: bool, root: bool) -> gtk::ComboBoxText {
    let gids: Vec<u32> = infos.iter().map(|i| i.attribute_uint32("unix::gid")).collect();
    let mut groups: Vec<(u32, String)> = if root { all_groups() } else { all_groups().into_iter().filter(|(g, _)| my_groups().contains(g)).collect() };
    for (gid, info) in gids.iter().zip(infos) {
        if !groups.iter().any(|(g, _)| g == gid) {
            groups.push((*gid, info.attribute_string("owner::group").map_or_else(|| gid.to_string(), |n| n.to_string())));
        }
    }
    groups.sort_by(|a, b| a.1.cmp(&b.1));
    let current = Some(gids[0]).filter(|g| gids.iter().all(|x| x == g));
    let combo = id_combo(dialog, files, &groups, current, usable, n_("Could not change the group"), |p, gid| std::os::unix::fs::chown(p, None, Some(gid)));
    if !usable {
        combo.set_tooltip_text(Some(&tr("Only the owner can change the group")));
    }
    combo
}

/// Every user (uid, "name" or "name – Full Name"), by name.
fn all_users() -> Vec<(u32, String)> {
    let mut out = vec![];
    // SAFETY: getpwent's results are copied out before the next call; on the
    // main thread only (nothing else here uses the passwd database).
    unsafe {
        libc::setpwent();
        loop {
            let pw = libc::getpwent();
            if pw.is_null() {
                break;
            }
            let name = std::ffi::CStr::from_ptr((*pw).pw_name).to_string_lossy().to_string();
            let gecos = if (*pw).pw_gecos.is_null() { String::new() } else { std::ffi::CStr::from_ptr((*pw).pw_gecos).to_string_lossy().to_string() };
            let full = gecos.split(',').next().unwrap_or("").trim().to_string();
            let label = if full.is_empty() || full == name { name } else { format!("{name} – {full}") };
            if !out.iter().any(|(u, _)| *u == (*pw).pw_uid) {
                out.push(((*pw).pw_uid, label));
            }
        }
        libc::endpwent();
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// Every group (gid, name).
fn all_groups() -> Vec<(u32, String)> {
    let mut out = vec![];
    // SAFETY: as in all_users.
    unsafe {
        libc::setgrent();
        loop {
            let gr = libc::getgrent();
            if gr.is_null() {
                break;
            }
            if !out.iter().any(|(g, _)| *g == (*gr).gr_gid) {
                out.push(((*gr).gr_gid, std::ffi::CStr::from_ptr((*gr).gr_name).to_string_lossy().to_string()));
            }
        }
        libc::endgrent();
    }
    out
}

/// The groups this process (the user) is in.
fn my_groups() -> Vec<u32> {
    // SAFETY: the buffer is as long as getgroups is told.
    unsafe {
        let n = libc::getgroups(0, std::ptr::null_mut());
        let mut v = vec![0 as libc::gid_t; n.max(0) as usize];
        let n = libc::getgroups(v.len() as i32, v.as_mut_ptr());
        v.truncate(n.max(0) as usize);
        v.push(libc::getegid());
        v
    }
}

/// Emblems: the icon theme's, each with a check box for the items, as in
/// Caja; kept as their keywords in metadata::emblems ("important" for
/// emblem-important).
fn emblems_page(dialog: &gtk::Dialog, files: &[gio::File], keywords: Vec<Vec<String>>) -> gtk::Widget {
    let keywords: Rc<Vec<std::cell::RefCell<Vec<String>>>> = Rc::new(keywords.into_iter().map(std::cell::RefCell::new).collect());
    // The theme's, and any the items have that it doesn't list there
    // (another theme's, or Caja's own), so they can be taken off.
    let mut emblems = icons::available_emblems();
    for k in keywords.iter().flat_map(|k| k.borrow().clone()) {
        let icon = format!("emblem-{k}");
        if !emblems.iter().any(|(i, _)| *i == icon) {
            let title = icons::emblem_title(&icon);
            emblems.push((icon, title));
        }
    }
    if emblems.is_empty() {
        let l = value_label(&tr("The icon theme has no emblems."));
        l.set_margin_start(12);
        l.set_margin_top(12);
        return l.upcast();
    }
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_max_children_per_line(3);
    flow.set_min_children_per_line(2);
    flow.set_homogeneous(true);
    flow.set_valign(gtk::Align::Start);
    flow.set_border_width(12);
    flow.set_row_spacing(4);
    flow.set_column_spacing(12);
    let files = Rc::new(files.to_vec());
    for (icon, title) in emblems {
        let keyword = icon.strip_prefix("emblem-").unwrap_or(&icon).to_string();
        let cb = gtk::CheckButton::new();
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let image = gtk::Image::from_icon_name(Some(&icon), gtk::IconSize::LargeToolbar);
        content.add(&image);
        content.add(&gtk::Label::new(Some(&title)));
        cb.add(&content);
        let with = keywords.iter().filter(|k| k.borrow().contains(&keyword)).count();
        cb.set_active(with == keywords.len());
        cb.set_inconsistent(with != 0 && with != keywords.len());
        let (keywords, files, d) = (keywords.clone(), files.clone(), dialog.clone());
        cb.connect_toggled(move |cb| {
            use crate::metadata::Value;
            cb.set_inconsistent(false);
            let on = cb.is_active();
            let mut writes = vec![];
            for (f, k) in files.iter().zip(keywords.iter()) {
                let mut k = k.borrow_mut();
                k.retain(|x| *x != keyword);
                if on {
                    k.push(keyword.clone());
                }
                writes.push((f.uri().to_string(), "emblems".to_string(), if k.is_empty() { Value::Unset } else { Value::List(k.clone()) }));
            }
            crate::metadata::write(writes, report(&d, tr("Could not change the emblems")));
            crate::window::refresh_everywhere(&files);
        });
        flow.add(&cb);
    }
    let sw = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    sw.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    sw.set_min_content_height(320);
    sw.add(&flow);
    sw.upcast()
}

/// Notes: a note kept with the item (metadata::annotation, as in Caja),
/// saved as it's typed. Items with one get the note emblem.
fn notes_page(dialog: &gtk::Dialog, file: &gio::File, note: &str) -> gtk::Widget {
    let view = gtk::TextView::new();
    file_drops_to_window(&view, dialog);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_left_margin(6);
    view.set_right_margin(6);
    view.set_top_margin(6);
    let buffer = view.buffer().expect("a text view has a buffer");
    buffer.set_text(note);
    let saved = Rc::new(std::cell::RefCell::new(buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).map(|t| t.to_string()).unwrap_or_default()));
    let save = {
        let (file, buffer, saved, d) = (file.clone(), buffer.clone(), saved, dialog.clone());
        Rc::new(move || {
            let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).map(|t| t.to_string()).unwrap_or_default();
            if *saved.borrow() == text {
                return;
            }
            use crate::metadata::Value;
            let value = if text.trim().is_empty() { Value::Unset } else { Value::Text(text.clone()) };
            crate::metadata::write(vec![(file.uri().to_string(), "annotation".to_string(), value)], report(&d, tr("Could not save the note")));
            *saved.borrow_mut() = text;
            crate::window::refresh_everywhere(std::slice::from_ref(&file));
        })
    };
    // Saved a moment after typing stops, and when the window closes.
    let pending: Rc<std::cell::Cell<Option<glib::SourceId>>> = Rc::default();
    {
        let (save, pending) = (save.clone(), pending.clone());
        buffer.connect_changed(move |_| {
            if let Some(id) = pending.take() {
                id.remove();
            }
            let (save, p) = (save.clone(), pending.clone());
            pending.set(Some(glib::timeout_add_local_once(std::time::Duration::from_millis(700), move || {
                p.take();
                save();
            })));
        });
    }
    // (Also when other items are shown in the window: the tab goes then.)
    view.connect_destroy(move |_| {
        if let Some(id) = pending.take() {
            id.remove();
        }
        save();
    });
    let sw = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    sw.set_shadow_type(gtk::ShadowType::In);
    sw.set_min_content_height(260);
    sw.add(&view);
    let page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    page.set_border_width(12);
    page.pack_start(&value_label(&tr("A note about this item, kept with it:")), false, false, 0);
    page.pack_start(&sw, true, true, 0);
    page.upcast()
}

fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
}

#[cfg(test)]
mod tests {
    #[test]
    fn web_links_have_no_properties() {
        use super::shows_properties;
        assert!(shows_properties("file:///home/u/a.mp4"));
        assert!(shows_properties("sftp://host/srv/a.txt"));
        assert!(shows_properties("trash:///a"));
        assert!(!shows_properties("https://example.org/a.html"));
        assert!(!shows_properties("HTTP://example.org/"));
    }

    #[test]
    fn digits_grouped_by_three() {
        for (n, want) in [(0, "0"), (999, "999"), (1000, "1,000"), (123456, "123,456"), (1234567, "1,234,567"), (u64::MAX, "18,446,744,073,709,551,615")] {
            assert_eq!(super::group_digits(n), want);
        }
    }

    #[test]
    fn parse_media_info() {
        let text = "General\nComplete name                            : /a/b c.mp4\nFormat                                   : MPEG-4\n\nVideo\nID                                       : 1\nFormat/Info                              : Advanced Video Codec : x\n\nEmpty\n";
        let s = super::parse_media_info(text);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].0, "General");
        assert_eq!(s[0].1, vec![("Complete name".into(), "/a/b c.mp4".into()), ("Format".into(), "MPEG-4".into())]);
        assert_eq!(s[1].1[1], ("Format/Info".to_string(), "Advanced Video Codec : x".to_string()));
    }

    #[test]
    fn random_media_info_text() {
        crate::testutil::cases(3000, |r| {
            let text = (0..r.below(8)).map(|_| r.text(30)).collect::<Vec<_>>().join(if r.chance(2) { " : " } else { "\n" });
            for (section, rows) in super::parse_media_info(&text) {
                assert!(!section.is_empty() && !rows.is_empty());
            }
        });
    }

    #[test]
    fn users_and_groups() {
        let me = unsafe { super::libc_getuid() };
        assert!(super::all_users().iter().any(|(u, _)| *u == me));
        let mine = super::my_groups();
        assert!(mine.contains(&unsafe { libc::getegid() }));
        let all = super::all_groups();
        assert!(mine.iter().all(|g| all.iter().any(|(x, _)| x == g) || *g == unsafe { libc::getegid() }));
    }

    #[test]
    fn apply_enclosed_group() {
        use std::os::unix::fs::MetadataExt;
        // Needs a second group of the user's to change to.
        let egid = unsafe { libc::getegid() };
        let Some(other) = super::my_groups().into_iter().find(|g| *g != egid) else { return };
        let t = crate::testutil::TestDir::new("props-enclosed-group");
        let f = t.file("top/sub/a.txt", "a");
        let top = t.0.join("top");
        std::os::unix::fs::chown(&top, None, Some(other)).unwrap();
        let (changed, failed) = super::apply_enclosed(&[(top.clone(), 0o755)], false);
        assert_eq!(failed, 0);
        assert!(changed >= 2);
        assert_eq!(std::fs::metadata(&f).unwrap().gid(), other);
        assert_eq!(std::fs::metadata(top.join("sub")).unwrap().gid(), other);
    }

    #[test]
    fn apply_enclosed_modes() {
        use std::os::unix::fs::PermissionsExt;
        let t = crate::testutil::TestDir::new("props-enclosed");
        let f = t.file("top/a.txt", "a");
        let x = t.file("top/run.sh", "x");
        let sub = t.0.join("top/sub");
        std::fs::create_dir_all(&sub).unwrap();
        let g = t.file("top/sub/b.txt", "b");
        let set = |p: &std::path::Path, m: u32| std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap();
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o7777;
        set(&f, 0o600);
        set(&x, 0o700);
        set(&g, 0o644);
        set(&sub, 0o700);
        std::os::unix::fs::symlink(&f, t.0.join("top/link")).unwrap();
        let (changed, failed) = super::apply_enclosed(&[(t.0.join("top"), 0o750)], false);
        assert_eq!(failed, 0);
        assert_eq!(mode(&sub), 0o750);
        assert_eq!(mode(&f), 0o640);
        // Execute bits stay the file's own.
        assert_eq!(mode(&x), 0o740);
        assert_eq!(mode(&g), 0o640);
        assert_eq!(changed, 4);
    }
}
