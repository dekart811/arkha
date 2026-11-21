//! What happens when a medium is inserted, as in Caja (caja-autorun.c):
//! for what it holds (an audio CD, a video DVD, a camera's photos, a music
//! player, software, …) start a program, open its folder, do nothing, or
//! ask. Only for media Arkha mounted itself when they were inserted: what a
//! desktop mounts, its own file manager or shell sees to.
//!
//! The choices are the desktop's media-handling settings with GNOME,
//! Cinnamon and MATE (so Caja's are Arkha's and the other way round), and
//! Arkha's own otherwise.

use crate::i18n::{fill, n_, tr};
use gio::prelude::*;
use gtk::prelude::*;

use crate::config;

/// What to do with a kind of content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Ask,
    Nothing,
    OpenFolder,
    /// The default program for the content type.
    StartApp,
}

// ---------------------------------------------------------------- settings

/// The settings' lists, by their desktop key.
const LISTS: [&str; 3] = ["autorun-x-content-start-app", "autorun-x-content-ignore", "autorun-x-content-open-folder"];

fn get_list(key: &str) -> Vec<String> {
    if let Some(s) = crate::sidebar::media_settings() {
        return s.strv(key).iter().map(|x| x.to_string()).collect();
    }
    config::with(|c| match key {
        "autorun-x-content-start-app" => c.autorun_start_app.clone(),
        "autorun-x-content-ignore" => c.autorun_ignore.clone(),
        _ => c.autorun_open_folder.clone(),
    })
}

fn set_list(key: &str, list: Vec<String>) {
    if let Some(s) = crate::sidebar::media_settings() {
        let refs: Vec<&str> = list.iter().map(|x| x.as_str()).collect();
        let _ = s.set_strv(key, refs.as_slice());
        gio::Settings::sync();
        return;
    }
    config::update(|c| match key {
        "autorun-x-content-start-app" => c.autorun_start_app = list,
        "autorun-x-content-ignore" => c.autorun_ignore = list,
        _ => c.autorun_open_folder = list,
    });
}

fn get_bool(key: &str) -> bool {
    if let Some(s) = crate::sidebar::media_settings() {
        return s.boolean(key);
    }
    config::with(|c| if key == "autorun-never" { c.autorun_never } else { c.automount_open })
}

fn set_bool(key: &str, on: bool) {
    if let Some(s) = crate::sidebar::media_settings() {
        let _ = s.set_boolean(key, on);
        gio::Settings::sync();
        return;
    }
    config::update(|c| if key == "autorun-never" { c.autorun_never = on } else { c.automount_open = on });
}

/// "Never prompt or start programs on media insertion".
pub fn never() -> bool {
    get_bool("autorun-never")
}

pub fn set_never(on: bool) {
    set_bool("autorun-never", on);
}

/// "Browse media when inserted": open the folder of media that hold
/// nothing in particular.
pub fn browse() -> bool {
    get_bool("automount-open")
}

pub fn set_browse(on: bool) {
    set_bool("automount-open", on);
}

/// What `lists` (start-app, ignore, open-folder) say for `ctype`.
fn action_in(lists: &[Vec<String>; 3], ctype: &str) -> Action {
    let has = |i: usize| lists[i].iter().any(|t| t == ctype);
    if has(0) {
        Action::StartApp
    } else if has(1) {
        Action::Nothing
    } else if has(2) {
        Action::OpenFolder
    } else {
        Action::Ask
    }
}

/// `lists` with `ctype` in the one for `action` only (in none for Ask).
fn with_action(mut lists: [Vec<String>; 3], ctype: &str, action: Action) -> [Vec<String>; 3] {
    for l in lists.iter_mut() {
        l.retain(|t| t != ctype);
    }
    let i = match action {
        Action::StartApp => Some(0),
        Action::Nothing => Some(1),
        Action::OpenFolder => Some(2),
        Action::Ask => None,
    };
    if let Some(i) = i {
        lists[i].push(ctype.to_string());
    }
    lists
}

pub fn action_for(ctype: &str) -> Action {
    action_in(&LISTS.map(get_list), ctype)
}

pub fn set_action(ctype: &str, action: Action) {
    let old = LISTS.map(get_list);
    let new = with_action(old.clone(), ctype, action);
    for (i, key) in LISTS.iter().enumerate() {
        if new[i] != old[i] {
            set_list(key, new[i].clone());
        }
    }
}

// ---------------------------------------------------------------- media

/// A medium as the flow needs it: a mount, or (to try it out, see main.rs)
/// a folder standing in for one.
pub struct Medium {
    pub name: String,
    pub icon: Option<gio::Icon>,
    pub root: gio::File,
    pub mount: Option<gio::Mount>,
}

/// After Arkha mounted a newly inserted medium.
pub fn on_mounted(mount: &gio::Mount) {
    if never() {
        return;
    }
    let medium = Medium { name: mount.name().to_string(), icon: Some(mount.icon()), root: mount.root(), mount: Some(mount.clone()) };
    mount.guess_content_type(false, gio::Cancellable::NONE, move |r| {
        let types: Vec<String> = r.map(|v| v.iter().map(|t| t.to_string()).collect()).unwrap_or_default();
        run(&medium, &types);
    });
}

/// The flow for `folder` as if it were a medium just inserted (for tests:
/// ARKHA_AUTORUN_TRY, see main.rs).
pub fn try_folder(folder: &gio::File) {
    let medium = Medium { name: crate::util::display_name(folder), icon: Some(gio::ThemedIcon::new("media-removable").upcast()), root: folder.clone(), mount: None };
    let types: Vec<String> = gio::content_type_guess_for_tree(folder).iter().map(|t| t.to_string()).collect();
    run(&medium, &types);
}

fn run(medium: &Medium, types: &[String]) {
    if never() {
        return;
    }
    let mut open = types.is_empty() && browse();
    for t in types {
        // Windows programs: never anything, as in Caja.
        if gio::content_type_is_a(t, "x-content/win32-software") {
            continue;
        }
        match action_for(t) {
            Action::StartApp => start_app(medium, gio::AppInfo::default_for_type(t, false)),
            Action::OpenFolder => open = true,
            Action::Nothing => {}
            Action::Ask => ask(medium, t),
        }
    }
    if open {
        open_folder(&medium.root);
    }
}

fn start_app(medium: &Medium, app: Option<gio::AppInfo>) {
    let Some(app) = app else { return };
    let ctx = gdk::Display::default().and_then(|d| d.app_launch_context());
    if let Err(e) = app.launch_uris(&[medium.root.uri().as_str()], ctx.as_ref()) {
        crate::util::error_dialog(None, &fill(&tr("Could not open “{name}”"), &[("name", &medium.name)]), e.message());
    }
}

fn open_folder(root: &gio::File) {
    if let Some(app) = gio::Application::default().and_downcast::<gtk::Application>() {
        crate::window::AppWindow::new(&app, std::slice::from_ref(root));
    }
}

/// Caja's words for the well-known kinds of media.
fn greeting(ctype: &str) -> String {
    match ctype {
        "x-content/audio-cdda" => tr("You have just inserted an Audio CD."),
        "x-content/audio-dvd" => tr("You have just inserted an Audio DVD."),
        "x-content/video-dvd" => tr("You have just inserted a Video DVD."),
        "x-content/video-vcd" => tr("You have just inserted a Video CD."),
        "x-content/video-svcd" => tr("You have just inserted a Super Video CD."),
        "x-content/blank-cd" => tr("You have just inserted a blank CD."),
        "x-content/blank-dvd" => tr("You have just inserted a blank DVD."),
        "x-content/blank-bd" => tr("You have just inserted a blank Blu-Ray disc."),
        "x-content/blank-hddvd" => tr("You have just inserted a blank HD DVD."),
        "x-content/image-photocd" => tr("You have just inserted a Photo CD."),
        "x-content/image-picturecd" => tr("You have just inserted a Picture CD."),
        "x-content/image-dcf" => tr("You have just inserted a medium with digital photos."),
        "x-content/audio-player" => tr("You have just inserted a digital audio player."),
        t if gio::content_type_is_a(t, "x-content/software") => tr("You have just inserted a medium with software intended to be automatically started."),
        _ => tr("You have just inserted a medium."),
    }
}

// ---------------------------------------------------------------- choices

/// A list of what can be done with `ctype`: ask (if `with_ask`), do
/// nothing, open the folder, open with one of the programs for it (the
/// default first), or with another program. Ids: "ask", "nothing",
/// "folder", "app:<id>", "other".
pub fn choices(ctype: &str, with_ask: bool) -> gtk::ComboBoxText {
    let c = gtk::ComboBoxText::new();
    if with_ask {
        c.append(Some("ask"), &tr("Ask what to do"));
    }
    c.append(Some("nothing"), &tr("Do Nothing"));
    c.append(Some("folder"), &tr("Open Folder"));
    let default = gio::AppInfo::default_for_type(ctype, false);
    let mut apps = gio::AppInfo::all_for_type(ctype);
    if let Some(d) = &default {
        apps.retain(|a| a.id() != d.id());
        apps.insert(0, d.clone());
    }
    for a in &apps {
        if let Some(id) = a.id() {
            // TRANSLATORS: what to do with a medium: "Open Rhythmbox".
            c.append(Some(&format!("app:{id}")), &fill(&tr("Open {program}"), &[("program", &a.display_name())]));
        }
    }
    c.append(Some("other"), &tr("Open with Other Application…"));
    // What it does now.
    let current = match action_for(ctype) {
        Action::Ask if with_ask => "ask".to_string(),
        Action::Ask | Action::Nothing => "nothing".to_string(),
        Action::OpenFolder => "folder".to_string(),
        Action::StartApp => default.and_then(|d| d.id()).map(|id| format!("app:{id}")).unwrap_or_else(|| "nothing".into()),
    };
    c.set_active_id(Some(&current));
    c
}

/// After "Open with Other Application…": the program picked is added to
/// `combo` and chosen; Cancel goes back to `previous`.
fn pick_other(combo: &gtk::ComboBoxText, ctype: &str, previous: Option<String>) {
    let parent = combo.toplevel().and_downcast::<gtk::Window>();
    let d = gtk::AppChooserDialog::for_content_type(parent.as_ref(), gtk::DialogFlags::MODAL, ctype);
    let picked = if d.run() == gtk::ResponseType::Ok { d.app_info() } else { None };
    d.close();
    match picked.and_then(|a| Some((a.id()?, a.display_name()))) {
        Some((id, name)) => {
            let key = format!("app:{id}");
            if !combo.set_active_id(Some(&key)) {
                combo.append(Some(&key), &fill(&tr("Open {program}"), &[("program", &name)]));
                combo.set_active_id(Some(&key));
            }
        }
        None => {
            combo.set_active_id(previous.as_deref());
        }
    }
}

/// Makes the choice in `combo` the one for `ctype`: a program becomes the
/// default for it.
fn remember(ctype: &str, id: &str) {
    match id {
        "ask" => set_action(ctype, Action::Ask),
        "nothing" => set_action(ctype, Action::Nothing),
        "folder" => set_action(ctype, Action::OpenFolder),
        a => {
            if let Some(app) = a.strip_prefix("app:").and_then(gio::DesktopAppInfo::new) {
                let _ = app.set_as_default_for_type(ctype);
                set_action(ctype, Action::StartApp);
            }
        }
    }
}

/// The question for a medium holding `ctype` (Caja's dialog).
fn ask(medium: &Medium, ctype: &str) {
    let d = gtk::Dialog::new();
    d.set_title(&medium.name);
    d.set_position(gtk::WindowPosition::Center);
    let hb = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    hb.set_border_width(12);
    let img = gtk::Image::new();
    if let Some(icon) = &medium.icon {
        img.set_from_gicon(icon, gtk::IconSize::Dialog);
    }
    img.set_valign(gtk::Align::Start);
    hb.pack_start(&img, false, false, 0);
    let vb = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title = gtk::Label::new(None);
    // TRANSLATORS: after "You have just inserted an Audio CD." and the like.
    let question = format!("{} {}", greeting(ctype), tr("Choose what application to launch."));
    title.set_markup(&format!("<big><b>{}</b></big>", glib::markup_escape_text(&question)));
    let text = gtk::Label::new(Some(&fill(
        &tr("Select how to open “{name}” and whether to perform this action in the future for other media of type “{type}”."),
        &[("name", &medium.name), ("type", &gio::content_type_get_description(ctype))],
    )));
    for l in [&title, &text] {
        l.set_line_wrap(true);
        l.set_max_width_chars(50);
        l.set_xalign(0.0);
        vb.pack_start(l, false, false, 0);
    }
    let combo = choices(ctype, true);
    // Asking again is no answer here: the default program, or the folder.
    if combo.active_id().as_deref() == Some("ask") {
        let first_app = gio::AppInfo::default_for_type(ctype, false).and_then(|a| a.id()).map(|id| format!("app:{id}"));
        if !first_app.is_some_and(|id| combo.set_active_id(Some(&id))) {
            combo.set_active_id(Some("folder"));
        }
    }
    vb.pack_start(&combo, false, false, 0);
    let always = gtk::CheckButton::with_mnemonic(&tr("_Always perform this action"));
    vb.pack_start(&always, false, false, 0);
    hb.pack_start(&vb, true, true, 0);
    d.content_area().pack_start(&hb, true, true, 0);
    let can_eject = medium.mount.as_ref().is_some_and(|m| m.can_eject());
    let eject = d.add_button(&if can_eject { tr("_Eject") } else { tr("_Unmount") }, gtk::ResponseType::Other(1));
    if medium.mount.is_none() {
        eject.set_sensitive(false);
    }
    if let Some(bb) = eject.parent().and_downcast::<gtk::ButtonBox>() {
        bb.set_child_secondary(&eject, true);
    }
    d.add_button(&tr("_Cancel"), gtk::ResponseType::Cancel);
    d.add_button(&tr("_OK"), gtk::ResponseType::Ok);
    d.set_default_response(gtk::ResponseType::Ok);
    {
        let ctype = ctype.to_string();
        let last = std::rc::Rc::new(std::cell::RefCell::new(combo.active_id().map(|s| s.to_string())));
        combo.connect_changed(move |c| {
            if c.active_id().as_deref() == Some("other") {
                let prev = last.borrow().clone();
                pick_other(c, &ctype, prev);
            }
            *last.borrow_mut() = c.active_id().map(|s| s.to_string());
        });
    }
    // Gone meanwhile (pulled out): no question left.
    if let Some(m) = &medium.mount {
        let d = d.clone();
        m.connect_unmounted(move |_| d.close());
    }
    let (root, name, mount, ctype) = (medium.root.clone(), medium.name.clone(), medium.mount.clone(), ctype.to_string());
    d.connect_response(move |d, resp| {
        match resp {
            gtk::ResponseType::Ok => {
                let id = combo.active_id().map(|s| s.to_string()).unwrap_or_default();
                if always.is_active() {
                    remember(&ctype, &id);
                }
                let m = Medium { name: name.clone(), icon: None, root: root.clone(), mount: mount.clone() };
                match id.as_str() {
                    "folder" => open_folder(&root),
                    a if a.starts_with("app:") => start_app(&m, a.strip_prefix("app:").and_then(gio::DesktopAppInfo::new).map(|a| a.upcast())),
                    _ => {}
                }
            }
            gtk::ResponseType::Other(1) => {
                if let Some(m) = &mount {
                    let op = gtk::MountOperation::new(Some(d));
                    if m.can_eject() {
                        m.eject_with_operation(gio::MountUnmountFlags::NONE, Some(&op), gio::Cancellable::NONE, |_| {});
                    } else {
                        m.unmount_with_operation(gio::MountUnmountFlags::NONE, Some(&op), gio::Cancellable::NONE, |_| {});
                    }
                }
            }
            _ => {}
        }
        d.close();
    });
    d.show_all();
    d.present();
}

// ---------------------------------------------------------------- preferences

/// The media Caja's Media tab lists by name, then the less common ones.
const COMMON: [(&str, &str); 5] = [
    ("x-content/audio-cdda", n_("CD _Audio:")),
    ("x-content/video-dvd", n_("_DVD Video:")),
    ("x-content/audio-player", n_("_Music Player:")),
    ("x-content/image-dcf", n_("_Photos:")),
    ("x-content/unix-software", n_("_Software:")),
];

/// Preferences → Media, as Caja's tab: a choice for each kind of medium,
/// and the two switches.
pub fn preferences_page() -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
    b.set_border_width(14);
    let head = |t: &str, first: bool| {
        let l = gtk::Label::new(None);
        l.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(t)));
        l.set_xalign(0.0);
        if !first {
            l.set_margin_top(8);
        }
        l
    };
    let note = |t: &str| {
        let l = gtk::Label::new(Some(t));
        l.set_xalign(0.0);
        l.set_line_wrap(true);
        l.set_max_width_chars(60);
        l.style_context().add_class("dim-label");
        l.set_margin_start(40);
        l
    };
    // Under the Removable Media section Preferences puts above.
    b.add(&head(&tr("Media Handling"), false));
    let where_ = match crate::sidebar::automount_desktop().filter(|_| crate::sidebar::media_settings().is_some()) {
        // TRANSLATORS: {desktop} is a desktop's name, such as MATE or GNOME.
        Some(desktop) => fill(&tr("What happens when you insert media or connect devices that Arkha mounts. These are {desktop}’s own settings."), &[("desktop", &desktop)]),
        None => tr("What happens when you insert media or connect devices that Arkha mounts."),
    };
    b.add(&note(&where_));
    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(12);
    grid.set_margin_start(40);
    let wire = |c: &gtk::ComboBoxText, ctype: std::rc::Rc<std::cell::RefCell<String>>| {
        let last = std::rc::Rc::new(std::cell::RefCell::new(c.active_id().map(|s| s.to_string())));
        c.connect_changed(move |c| {
            let t = ctype.borrow().clone();
            if c.active_id().as_deref() == Some("other") {
                let prev = last.borrow().clone();
                pick_other(c, &t, prev);
            }
            let id = c.active_id().map(|s| s.to_string()).unwrap_or_default();
            if id != "other" && last.borrow().as_deref() != Some(id.as_str()) {
                remember(&t, &id);
            }
            *last.borrow_mut() = Some(id);
        });
    };
    for (r, (ctype, label)) in COMMON.iter().enumerate() {
        let l = gtk::Label::with_mnemonic(&tr(label));
        l.set_xalign(0.0);
        let c = choices(ctype, true);
        c.set_hexpand(true);
        l.set_mnemonic_widget(Some(&c));
        wire(&c, std::rc::Rc::new(std::cell::RefCell::new(ctype.to_string())));
        grid.attach(&l, 0, r as i32, 1, 1);
        grid.attach(&c, 1, r as i32, 1, 1);
    }
    b.add(&grid);
    // Other media: pick the kind, then what to do with it.
    b.add(&head(&tr("Other Media"), false));
    b.add(&note(&tr("Less common media formats can be configured here.")));
    let others = gtk::Grid::new();
    others.set_row_spacing(6);
    others.set_column_spacing(12);
    others.set_margin_start(40);
    let kinds = gtk::ComboBoxText::new();
    let mut types: Vec<(String, String)> = gio::content_types_get_registered()
        .iter()
        .map(|t| t.to_string())
        .filter(|t| t.starts_with("x-content/") && !COMMON.iter().any(|(c, _)| c == t) && !gio::content_type_is_a(t, "x-content/win32-software") && t != "x-content/software")
        .map(|t| {
            let d = gio::content_type_get_description(&t).to_string();
            (t, d)
        })
        .collect();
    types.sort_by_key(|t| t.1.to_lowercase());
    for (t, d) in &types {
        kinds.append(Some(t), d);
    }
    kinds.set_active(Some(0));
    kinds.set_hexpand(true);
    let action_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    action_box.set_hexpand(true);
    let current_type = std::rc::Rc::new(std::cell::RefCell::new(kinds.active_id().map(|s| s.to_string()).unwrap_or_default()));
    let rebuild = {
        let (action_box, current_type) = (action_box.clone(), current_type.clone());
        move |t: &str| {
            *current_type.borrow_mut() = t.to_string();
            for w in action_box.children() {
                action_box.remove(&w);
            }
            let c = choices(t, true);
            c.set_hexpand(true);
            wire(&c, current_type.clone());
            action_box.pack_start(&c, true, true, 0);
            action_box.show_all();
        }
    };
    let first = current_type.borrow().clone();
    rebuild(&first);
    kinds.connect_changed(move |k| {
        if let Some(t) = k.active_id() {
            rebuild(&t);
        }
    });
    let tl = gtk::Label::with_mnemonic(&tr("_Type:"));
    tl.set_xalign(0.0);
    tl.set_mnemonic_widget(Some(&kinds));
    let al = gtk::Label::new(Some(&tr("Action:")));
    al.set_xalign(0.0);
    others.attach(&tl, 0, 0, 1, 1);
    others.attach(&kinds, 1, 0, 1, 1);
    others.attach(&al, 0, 1, 1, 1);
    others.attach(&action_box, 1, 1, 1, 1);
    b.add(&others);
    b.add(&head(&tr("When Media Are Inserted"), false));
    let never_cb = gtk::CheckButton::with_mnemonic(&tr("_Never prompt or start programs on media insertion"));
    never_cb.set_active(never());
    never_cb.connect_toggled(|c| set_never(c.is_active()));
    never_cb.set_margin_start(40);
    b.add(&never_cb);
    let browse_cb = gtk::CheckButton::with_mnemonic(&tr("B_rowse media when inserted"));
    browse_cb.set_tooltip_text(Some(&tr("Open a window for media that hold nothing in particular (no photos, music, video…)")));
    browse_cb.set_active(browse());
    browse_cb.connect_toggled(|c| set_browse(c.is_active()));
    browse_cb.set_margin_start(40);
    b.add(&browse_cb);
    // The choices mean nothing while nothing is ever started.
    for w in [grid.upcast_ref::<gtk::Widget>(), others.upcast_ref(), browse_cb.upcast_ref()] {
        never_cb.bind_property("active", w, "sensitive").invert_boolean().sync_create().build();
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_say_what_to_do() {
        let none: [Vec<String>; 3] = Default::default();
        assert_eq!(action_in(&none, "x-content/image-dcf"), Action::Ask);
        let l = with_action(none, "x-content/image-dcf", Action::StartApp);
        assert_eq!(action_in(&l, "x-content/image-dcf"), Action::StartApp);
        // One list at a time: a new choice takes it out of the others.
        let l = with_action(l, "x-content/image-dcf", Action::OpenFolder);
        assert_eq!(l, [vec![], vec![], vec!["x-content/image-dcf".to_string()]]);
        let l = with_action(l, "x-content/audio-cdda", Action::Nothing);
        assert_eq!(action_in(&l, "x-content/audio-cdda"), Action::Nothing);
        assert_eq!(action_in(&l, "x-content/image-dcf"), Action::OpenFolder);
        let l = with_action(l, "x-content/image-dcf", Action::Ask);
        assert_eq!(action_in(&l, "x-content/image-dcf"), Action::Ask);
        assert_eq!(l[2], Vec::<String>::new());
    }

    #[test]
    fn camera_folders_are_photos() {
        // GIO knows a camera's card by its DCIM folder.
        let t = crate::testutil::TestDir::new("autorun-dcim");
        t.file("DCIM/100CANON/IMG_0001.JPG", "x");
        let types: Vec<String> = gio::content_type_guess_for_tree(&gio::File::for_path(&t.0)).iter().map(|t| t.to_string()).collect();
        assert!(types.iter().any(|t| t == "x-content/image-dcf"), "{types:?}");
    }
}
