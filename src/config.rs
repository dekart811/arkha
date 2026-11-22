//! Persistent settings in ~/.config/arkha/arkha.conf

use std::cell::RefCell;

/// The settings, each declared once: its name (also its key in the file's
/// [General] group), type, default and what it's for. The macro makes the
/// struct, its defaults, and reading and writing them all (see Stored);
/// what a setting needs checked after reading is in `checked`.
macro_rules! settings {
    ($( $(#[$m:meta])* $name:ident : $ty:ty = $default:expr ),* $(,)?) => {
        #[derive(Clone, Debug)]
        pub struct Settings {
            $( $(#[$m])* pub $name: $ty, )*
        }

        impl Default for Settings {
            fn default() -> Self {
                Settings { $( $name: $default, )* }
            }
        }

        /// Every setting `kf` has (each missing or unreadable one is left
        /// as it is in `s`).
        fn read_all(kf: &glib::KeyFile, s: &mut Settings) {
            $( if let Some(v) = <$ty as Stored>::read(kf, stringify!($name)) {
                s.$name = v;
            } )*
        }

        /// Every setting, into `kf`.
        fn write_all(kf: &glib::KeyFile, s: &Settings) {
            $( Stored::write(&s.$name, kf, stringify!($name)); )*
        }
    };
}

/// How a setting's type is kept in the file, under its key in [General].
trait Stored: Sized {
    fn read(kf: &glib::KeyFile, key: &str) -> Option<Self>;
    fn write(&self, kf: &glib::KeyFile, key: &str);
}

const GROUP: &str = "General";

impl Stored for bool {
    fn read(kf: &glib::KeyFile, key: &str) -> Option<Self> {
        kf.boolean(GROUP, key).ok()
    }
    fn write(&self, kf: &glib::KeyFile, key: &str) {
        kf.set_boolean(GROUP, key, *self);
    }
}

impl Stored for i32 {
    fn read(kf: &glib::KeyFile, key: &str) -> Option<Self> {
        kf.integer(GROUP, key).ok()
    }
    fn write(&self, kf: &glib::KeyFile, key: &str) {
        kf.set_integer(GROUP, key, *self);
    }
}

impl Stored for u64 {
    fn read(kf: &glib::KeyFile, key: &str) -> Option<Self> {
        kf.uint64(GROUP, key).ok()
    }
    fn write(&self, kf: &glib::KeyFile, key: &str) {
        kf.set_uint64(GROUP, key, *self);
    }
}

impl Stored for String {
    fn read(kf: &glib::KeyFile, key: &str) -> Option<Self> {
        kf.string(GROUP, key).ok().map(|v| v.to_string())
    }
    fn write(&self, kf: &glib::KeyFile, key: &str) {
        kf.set_string(GROUP, key, self);
    }
}

impl Stored for Vec<String> {
    fn read(kf: &glib::KeyFile, key: &str) -> Option<Self> {
        kf.string_list(GROUP, key).ok().map(|v| v.iter().map(|x| x.to_string()).collect())
    }
    fn write(&self, kf: &glib::KeyFile, key: &str) {
        set_string_list(kf, GROUP, key, self);
    }
}

/// Zoom levels: only valid ones (see parse_levels), else the default stays.
impl Stored for Vec<i32> {
    fn read(kf: &glib::KeyFile, key: &str) -> Option<Self> {
        kf.integer_list(GROUP, key).ok().and_then(|v| parse_levels(&format_levels(&v)))
    }
    fn write(&self, kf: &glib::KeyFile, key: &str) {
        kf.set_value(GROUP, key, &self.iter().map(|l| format!("{l};")).collect::<String>());
    }
}

/// The keyboard shortcuts changed: a group of their own, [Keybindings],
/// action = its shortcuts (the key is unused).
impl Stored for Vec<(String, Vec<String>)> {
    fn read(kf: &glib::KeyFile, _: &str) -> Option<Self> {
        let keys = kf.keys("Keybindings").ok()?;
        let accels = |k: &str| kf.string_list("Keybindings", k).map(|v| v.iter().map(|a| a.to_string()).filter(|a| !a.is_empty()).collect()).unwrap_or_default();
        Some(keys.iter().map(|k| (k.to_string(), accels(k))).collect())
    }
    fn write(&self, kf: &glib::KeyFile, _: &str) {
        for (action, accels) in self {
            set_string_list(kf, "Keybindings", action, accels);
        }
    }
}

settings! {
    show_hidden: bool = false,
    /// "icon" | "list" | "compact"
    default_view: String = "icon".into(),
    /// Current zoom of each view in pixels; always one of its zoom levels.
    icon_size: i32 = NORMAL_ICON,
    list_size: i32 = NORMAL_LIST,
    compact_size: i32 = NORMAL_COMPACT,
    /// The sizes Zoom In / Zoom Out step through, ascending.
    icon_levels: Vec<i32> = ICON_SIZES.to_vec(),
    list_levels: Vec<i32> = LIST_SIZES.to_vec(),
    compact_levels: Vec<i32> = COMPACT_SIZES.to_vec(),
    /// "name" | "size" | "type" | "mtime"
    sort_by: String = "name".into(),
    sort_desc: bool = false,
    folders_first: bool = true,
    width: i32 = 1000,
    height: i32 = 650,
    maximized: bool = false,
    sidebar_width: i32 = 190,
    show_sidebar: bool = true,
    show_statusbar: bool = true,
    show_toolbar: bool = true,
    /// The mouse's extra buttons go back and forward. As in Caja (whose
    /// dconf keys these are), only in the settings file, not in Preferences.
    mouse_extra_buttons: bool = true,
    mouse_back_button: i32 = 8,
    mouse_forward_button: i32 = 9,
    /// Icons beside the names in the list view.
    list_icons: bool = true,
    /// The location bar (the path, the bar beside it). Hidden, it still
    /// shows while the location field or the search is open.
    show_locbar: bool = true,
    /// The bar beside the path can't be rearranged: no accidental drags
    /// (on a touchpad, say).
    lock_bar: bool = true,
    /// The status bar says how many items are hidden.
    status_hidden: bool = false,
    /// Show the tab bar even with one tab (so that tab can be dragged).
    always_show_tabs: bool = false,
    /// Text next to the icons of toolbar buttons (Back, Forward).
    toolbar_text: bool = false,
    /// Shell that runs custom context menu actions, as `<shell> -c <command>`.
    action_shell: String = "sh".into(),
    /// The archive manager for Extract Here and Compress (its command, see
    /// util::ARCHIVERS), or "" for the one set for archives (see
    /// util::archiver_command).
    archiver: String = String::new(),
    /// In split view, the pane under the mouse pointer gets focus.
    focus_follows_mouse: bool = false,
    /// How long (ms) the pointer has to stay over a pane before it takes the
    /// focus: passing over it on the way elsewhere doesn't count.
    focus_follows_mouse_delay: i32 = 300,
    /// Which pane stays when split view is turned off: "active" (the one
    /// in use) or "other".
    split_keep: String = "active".into(),
    /// Where the extra pane opens: "same" (the folder of the pane in use),
    /// "home", "last" (where the closed pane was, `split_last`) or
    /// "folder" (`split_folder`).
    split_opens: String = "same".into(),
    /// The folder (a URI) for split_opens "folder".
    split_folder: String = String::new(),
    /// The folder (a URI) the pane that closed with split view last showed.
    split_last: String = String::new(),
    /// Where New Tab opens, as split_opens: "same" (the current tab's
    /// folder), "home", "last" (`tab_last`) or "folder" (`tab_folder`).
    tab_opens: String = "same".into(),
    tab_folder: String = String::new(),
    /// The folder (a URI) of the tab closed last.
    tab_last: String = String::new(),
    /// Where new tabs go: "in-order" (as Firefox puts them), "after" (right
    /// after the current tab) or "end".
    new_tabs: String = "in-order".into(),
    /// Button groups at the right end of the location bar.
    show_zoom_buttons: bool = true,
    show_view_buttons: bool = true,
    show_split_button: bool = true,
    /// The Swap Panes button beside it (shown in split view).
    show_swap_button: bool = true,
    show_info_button: bool = true,
    /// Order of the location bar items (see BAR_ITEMS); rearranged by dragging.
    bar_order: Vec<String> = BAR_ITEMS.iter().map(|g| g.to_string()).collect(),
    /// Order of the bulk rename window's tabs (see RENAME_TABS); rearranged by dragging.
    rename_tab_order: Vec<String> = RENAME_TABS.iter().map(|t| t.to_string()).collect(),
    /// Order of the Properties window's tabs (see PROPS_TABS), the same way.
    props_tab_order: Vec<String> = PROPS_TABS.iter().map(|t| t.to_string()).collect(),
    /// The Properties tabs never shown (Preferences → Properties); never
    /// "basic", so the window always has a tab.
    hidden_props_tabs: Vec<String> = vec![],
    /// The checksums the Checksums tab offers: all of them in their order
    /// (ids as checksums::Algo::id), and those not offered.
    checksum_order: Vec<String> = crate::checksums::ALL.iter().map(|a| a.id().to_string()).collect(),
    hidden_checksums: Vec<String> = crate::checksums::HIDDEN_AT_FIRST.iter().map(|t| t.to_string()).collect(),
    /// Order of the side pane's sections (see SIDEBAR_SECTIONS); rearranged
    /// by dragging their headings.
    sidebar_sections: Vec<String> = SIDEBAR_SECTIONS.iter().map(|t| t.to_string()).collect(),
    /// The list view's columns after the name, in order (see details.rs).
    list_columns: Vec<String> = crate::details::DEFAULT_COLUMNS.iter().map(|t| t.to_string()).collect(),
    /// What shows under the names in the icon view, the more of them the
    /// bigger the icons (see details::shown_captions); "none" leaves a gap.
    icon_captions: Vec<String> = crate::details::DEFAULT_CAPTIONS.iter().map(|t| t.to_string()).collect(),
    /// Side pane sections showing only their heading.
    sidebar_collapsed: Vec<String> = vec![],
    /// Mount removable media when inserted (see sidebar::init_automount);
    /// only used without a desktop, whose own setting this is otherwise.
    automount: bool = true,
    /// The side pane's eject button only unmounts (no eject / power off).
    unmount_only: bool = false,
    /// Separate automounters (udiskie, …) already told about, once each.
    automounters_noticed: Vec<String> = vec![],
    /// What happens with media Arkha mounts when inserted (see autorun.rs),
    /// as the desktop's media-handling keys say, which are these otherwise
    /// (without one of GNOME, Cinnamon or MATE).
    autorun_never: bool = false,
    automount_open: bool = true,
    autorun_start_app: Vec<String> = vec![],
    autorun_ignore: Vec<String> = vec![],
    autorun_open_folder: Vec<String> = vec![],
    /// Always show the location as editable text instead of breadcrumbs.
    path_as_text: bool = false,
    confirm_trash: bool = true,
    /// File operations in their own window (opened when one takes a while)
    /// instead of the location bar's button and its popover.
    ops_window: bool = false,
    /// A new search looks in the subfolders too (each search can change it).
    search_subfolders: bool = true,
    single_click: bool = false,
    /// Previews, as Caja has them, each "always", "local" (local files
    /// only: not GVfs places such as network shares) or "never" (see
    /// util::preview_on): thumbnails, a text file's first lines in its icon,
    /// and how many items a folder has.
    show_thumbnails: String = "local".into(),
    /// Colours added in Backgrounds and Emblems ("#rrggbb").
    own_colors: Vec<String> = vec![],
    /// The side pane's background: a colour ("#rrggbb") and/or a picture's
    /// URI (see backgrounds.rs); empty for the theme's.
    side_pane_color: String = String::new(),
    side_pane_image: String = String::new(),
    text_in_icons: String = "local".into(),
    count_items: String = "local".into(),
    /// Images bigger than this (bytes) get no thumbnail (other files'
    /// thumbnailers decide for themselves), as in Caja.
    thumbnail_limit: u64 = 100 << 20,
    /// Side pane entries the user hid, by URI (or "label:Name" for unmounted drives).
    hidden_places: Vec<String> = vec![],
    /// The Computer entries' order in the side pane, by the same keys
    /// (empty: the usual order; see sidebar::ordered_places).
    places_order: Vec<String> = vec![],
    /// Built-in context menu items the user hid (keys from actions.rs).
    hidden_menu_items: Vec<String> = vec![],
    /// Order of the context menu for selected items and for a folder's
    /// empty space (see actions::menu_order); empty = the default.
    menu_order: Vec<String> = vec![],
    menu_order_bg: Vec<String> = vec![],
    /// Height of side pane rows in pixels; 0 = whatever the theme uses.
    sidebar_row_height: i32 = 0,
    /// Remember view and sorting per folder (see folders.rs).
    per_folder: bool = true,
    /// Alternating row colors in the list view.
    striped_rows: bool = false,
    /// The same in the bulk rename window's file list.
    rename_striped_rows: bool = true,
    /// File sizes in IEC units (KiB, MiB) instead of SI (kB, MB); see
    /// util::format_size.
    iec_units: bool = false,
    /// How dates are shown: "informal" (Today 14:00), "locale" or "iso".
    date_format: String = "informal".into(),
    /// The language of Arkha's texts (a translation's code, "en"), or ""
    /// for the system's; applies when Arkha starts (see i18n).
    language: String = String::new(),
    /// Folder covers (see info.rs): the file names looked for, ';'-separated
    /// and in order of preference, matched ignoring case.
    cover_names: String = "folder.jpg; folder.png; cover.jpg; cover.png; front.jpg".into(),
    /// What the info panel shows while several items are selected:
    /// "summary" (their number and total size), "last" (the last item),
    /// "current" (the open folder) or "hide" (the panel hides).
    info_several: String = "summary".into(),
    /// For a selected folder without a cover: "icon" (a folder icon and
    /// the details) or "none" (nothing).
    info_no_cover: String = "icon".into(),
    info_details: bool = true,
    /// The info panel when folders don't remember their own settings.
    info_panel: bool = false,
    info_panel_width: i32 = 240,
    /// Middle-click on empty space (and on files in the list view) starts
    /// autoscrolling (see scroll.rs).
    autoscroll: bool = true,
    /// "Media Info" tab (mediainfo output) in the properties dialog, and the
    /// kinds of files it's shown for.
    media_info: bool = true,
    media_info_video: bool = true,
    media_info_audio: bool = true,
    media_info_image: bool = true,
    media_info_other: bool = false,
    /// Shortcuts the user changed: (action, accelerators); an empty list
    /// means no shortcut. Actions not listed use their defaults (keys.rs).
    keybindings: Vec<(String, Vec<String>)> = vec![],
}

/// The bulk rename window's tabs, in their default order.
pub const RENAME_TABS: [&str; 6] = ["case", "insert", "remove", "replace", "enumerate", "regex"];
/// The Properties window's tabs (not every one shows for every item).
pub const PROPS_TABS: [&str; 7] = ["basic", "emblems", "permissions", "open-with", "checksums", "notes", "media"];
pub const SIDEBAR_SECTIONS: [&str; 3] = ["computer", "devices", "bookmarks"];

/// The location bar's button groups: zoom, view mode, split view, the info
/// panel, and the file operations button.
pub const BAR_GROUPS: [&str; 5] = ["zoom", "view", "split", "info", "ops"];
/// Everything in the location bar that `bar_order` arranges: the path
/// (breadcrumbs or text) and the button groups, in their default order.
pub const BAR_ITEMS: [&str; 6] = ["path", "ops", "split", "view", "zoom", "info"];

// Default zoom levels.
pub const ICON_SIZES: [i32; 8] = [24, 32, 48, 64, 96, 128, 192, 256];
pub const LIST_SIZES: [i32; 5] = [16, 24, 32, 48, 64];
pub const COMPACT_SIZES: [i32; 4] = [16, 24, 32, 48];
/// What "Normal Size" (Ctrl+0) goes back to, before snapping to a level.
pub const NORMAL_ICON: i32 = 48;
pub const NORMAL_LIST: i32 = 24;
pub const NORMAL_COMPACT: i32 = 24;
const MIN_LEVEL: i32 = 8;
const MAX_LEVEL: i32 = 512;

/// The level closest to `size` (the smaller one on a tie).
pub fn snap(levels: &[i32], size: i32) -> i32 {
    levels.iter().copied().min_by_key(|l| ((l - size).abs(), *l)).unwrap_or(size)
}

/// The next level above (`delta` > 0) or below `size`, staying put at the ends.
pub fn step(levels: &[i32], size: i32, delta: i32) -> i32 {
    let next = if delta > 0 {
        levels.iter().copied().find(|&l| l > size)
    } else {
        levels.iter().rev().copied().find(|&l| l < size)
    };
    next.unwrap_or(size)
}

/// Parses "24, 32 48" into sorted, de-duplicated levels; None if there are no
/// numbers or one is out of range or not a number.
pub fn parse_levels(text: &str) -> Option<Vec<i32>> {
    let mut v = vec![];
    for t in text.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|t| !t.is_empty()) {
        let n: i32 = t.trim_end_matches("px").parse().ok()?;
        if !(MIN_LEVEL..=MAX_LEVEL).contains(&n) {
            return None;
        }
        v.push(n);
    }
    v.sort_unstable();
    v.dedup();
    (!v.is_empty()).then_some(v)
}

pub fn format_levels(levels: &[i32]) -> String {
    levels.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", ")
}


thread_local! {
    static SETTINGS: RefCell<Settings> = RefCell::new(load());
}

/// Where settings are kept (~/.config). For the unit tests a folder in
/// target/, so they never read or change the real ones.
pub fn config_home() -> std::path::PathBuf {
    #[cfg(not(test))]
    return glib::user_config_dir();
    #[cfg(test)]
    return std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("test-config");
}

/// A file in Arkha's own settings folder.
pub fn arkha_file(name: &str) -> std::path::PathBuf {
    config_home().join("arkha").join(name)
}

fn path() -> std::path::PathBuf {
    arkha_file("arkha.conf")
}

fn load() -> Settings {
    let Some(kf) = crate::util::load_settings(&path()) else { return Settings::default() };
    from_keyfile(&kf)
}

/// The settings in a config file; anything missing or invalid keeps its default.
fn from_keyfile(kf: &glib::KeyFile) -> Settings {
    let mut s = Settings::default();
    read_all(kf, &mut s);
    checked(s)
}

/// Settings as read, made valid: each zoom size one of its levels, the
/// orders of known items only (each once, any missing at the end, or for
/// the bar after the one it follows), the lists of known items only, the
/// choices among their values, sizes in their range. Defaults are left as
/// they are.
fn checked(mut s: Settings) -> Settings {
    s.icon_size = snap(&s.icon_levels, s.icon_size);
    s.list_size = snap(&s.list_levels, s.list_size);
    s.compact_size = snap(&s.compact_levels, s.compact_size);
    // Known ones once each; any missing (e.g. added later) go at the end.
    let ordered = |v: &[String], known: &[&str]| -> Vec<String> {
        let mut order: Vec<String> = vec![];
        for id in v.iter().cloned().chain(known.iter().map(|t| t.to_string())) {
            if known.contains(&id.as_str()) && !order.contains(&id) {
                order.push(id);
            }
        }
        order
    };
    s.rename_tab_order = ordered(&s.rename_tab_order, &RENAME_TABS);
    s.props_tab_order = ordered(&s.props_tab_order, &PROPS_TABS);
    let mut hidden: Vec<String> = vec![];
    for t in &s.hidden_props_tabs {
        if t != "basic" && PROPS_TABS.contains(&t.as_str()) && !hidden.contains(t) {
            hidden.push(t.clone());
        }
    }
    s.hidden_props_tabs = hidden;
    let checksum_ids: Vec<&str> = crate::checksums::ALL.iter().map(|a| a.id()).collect();
    s.checksum_order = ordered(&s.checksum_order, &checksum_ids);
    s.hidden_checksums.retain(|id| checksum_ids.contains(&id.as_str()));
    s.list_columns = crate::details::known(&s.list_columns).iter().map(|t| t.to_string()).collect();
    // Three places; any not known are gaps.
    let captions = std::mem::take(&mut s.icon_captions);
    s.icon_captions = (0..3)
        .map(|i| captions.get(i).filter(|c| crate::details::known(std::slice::from_ref(c)).len() == 1).cloned().unwrap_or_else(|| "none".into()))
        .collect();
    s.sidebar_sections = ordered(&s.sidebar_sections, &SIDEBAR_SECTIONS);
    s.sidebar_collapsed.retain(|x| SIDEBAR_SECTIONS.contains(&x.as_str()));
    // The bar: known ids once each; missing ones (items added since it was
    // saved) go after the one they follow by default, or first.
    let mut order: Vec<String> = vec![];
    for id in &s.bar_order {
        if BAR_ITEMS.contains(&id.as_str()) && !order.contains(id) {
            order.push(id.clone());
        }
    }
    for (i, id) in BAR_ITEMS.iter().enumerate() {
        if order.iter().any(|o| o == id) {
            continue;
        }
        let at = BAR_ITEMS[..i].iter().rev().find_map(|prev| order.iter().position(|o| o == prev)).map_or(0, |p| p + 1);
        order.insert(at, id.to_string());
    }
    s.bar_order = order;
    let defaults = Settings::default();
    for (field, default) in [
        (&mut s.show_thumbnails, &defaults.show_thumbnails),
        (&mut s.text_in_icons, &defaults.text_in_icons),
        (&mut s.count_items, &defaults.count_items),
    ] {
        if !["always", "local", "never"].contains(&field.as_str()) {
            *field = default.clone();
        }
    }
    s.own_colors.retain(|c| gdk::RGBA::parse(c).is_ok());
    s.sidebar_row_height = s.sidebar_row_height.clamp(0, 200);
    s.info_panel_width = s.info_panel_width.clamp(120, 1200);
    s
}

pub fn get() -> Settings {
    SETTINGS.with(|s| s.borrow().clone())
}

/// A look at the settings without copying them all, for what runs often
/// (for every item, every redraw).
pub fn with<R>(f: impl FnOnce(&Settings) -> R) -> R {
    SETTINGS.with(|s| f(&s.borrow()))
}

pub fn update(f: impl FnOnce(&mut Settings)) {
    SETTINGS.with(|s| f(&mut s.borrow_mut()));
    save();
}

/// Changes the settings for the tests on this thread only, without saving
/// them (a file all the tests share).
#[cfg(test)]
pub fn set_here(f: impl FnOnce(&mut Settings)) {
    SETTINGS.with(|s| f(&mut s.borrow_mut()));
}

/// glib-rs has no set_string_list; write the list in KeyFile syntax.
fn set_string_list(kf: &glib::KeyFile, group: &str, key: &str, list: &[String]) {
    let value: String = list
        .iter()
        .map(|x| {
            let e = x
                .replace('\\', "\\\\")
                .replace(';', "\\;")
                .replace('\n', "\\n")
                .replace('\t', "\\t")
                .replace('\r', "\\r");
            match e.strip_prefix(' ') {
                Some(rest) => format!("\\s{rest};"),
                None => format!("{e};"),
            }
        })
        .collect();
    kf.set_value(group, key, &value);
}

fn save() {
    let kf = to_keyfile(&get());
    crate::util::save_settings(&kf, &path());
}

fn to_keyfile(s: &Settings) -> glib::KeyFile {
    let kf = glib::KeyFile::new();
    write_all(&kf, s);
    kf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_leave_the_defaults() {
        // (They run on every setting now, read or not.)
        let d = Settings::default();
        assert_eq!(to_keyfile(&checked(d.clone())).to_data(), to_keyfile(&d).to_data());
        assert_eq!(to_keyfile(&from_keyfile(&glib::KeyFile::new())).to_data(), to_keyfile(&d).to_data(), "an empty file: the defaults");
    }

    #[test]
    fn levels() {
        assert_eq!(parse_levels("64, 24 32;24px, 40"), Some(vec![24, 32, 40, 64]));
        assert_eq!(parse_levels(""), None);
        assert_eq!(parse_levels("24, big"), None);
        assert_eq!(parse_levels("4, 24"), None);
        assert_eq!(parse_levels("0"), None);
        assert_eq!(parse_levels("0, 24"), None);
        assert_eq!(parse_levels("-24"), None);
        assert_eq!(parse_levels("24, -48"), None);
        assert_eq!(parse_levels("24 - 48"), None);
        assert_eq!(format_levels(&[24, 32]), "24, 32");
        let l = [24, 32, 48];
        assert_eq!(step(&l, 32, 1), 48);
        assert_eq!(step(&l, 48, 1), 48);
        assert_eq!(step(&l, 24, -1), 24);
        assert_eq!(step(&l, 40, -1), 32);
        assert_eq!(snap(&l, 40), 32);
        assert_eq!(snap(&l, 44), 48);
        assert_eq!(snap(&l, 100), 48);
    }

    /// Each setting's name and value, as Debug shows them.
    fn fields(s: &Settings) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = vec![];
        for line in format!("{s:#?}").lines() {
            match line.strip_prefix("    ").filter(|l| !l.starts_with(' ')).and_then(|l| l.split_once(": ")) {
                Some((k, v)) => out.push((k.to_string(), v.trim_end_matches(',').to_string())),
                // The rest of a list.
                None => {
                    if let Some(last) = out.last_mut() {
                        last.1.push_str(line.trim());
                    }
                }
            }
        }
        out
    }

    #[test]
    fn every_setting_is_saved() {
        let def = Settings::default();
        let kf = to_keyfile(&def);
        let names = fields(&def);
        assert!(names.len() > 50);
        for (k, _) in names.iter().filter(|(k, _)| k != "keybindings") {
            assert!(kf.has_key("General", k).unwrap_or(false), "{k} isn't saved");
        }
    }

    #[test]
    fn every_setting_is_loaded() {
        let def = Settings::default();
        let other = |levels: &[i32], cur: &str| levels.iter().find(|l| l.to_string() != cur).unwrap().to_string();
        let list = |v: &[i32]| v.iter().map(|l| format!("{l};")).collect::<String>();
        for (k, v) in fields(&def).iter().filter(|(k, _)| k != "keybindings") {
            let kf = to_keyfile(&def);
            // Something valid that differs from the default.
            let new = match (k.as_str(), v.as_str()) {
                (_, "true") => "false".to_string(),
                (_, "false") => "true".to_string(),
                ("icon_size", _) => other(&def.icon_levels, v),
                ("list_size", _) => other(&def.list_levels, v),
                ("compact_size", _) => other(&def.compact_levels, v),
                ("icon_levels", _) => list(&def.icon_levels[1..]),
                ("list_levels", _) => list(&def.list_levels[1..]),
                ("compact_levels", _) => list(&def.compact_levels[1..]),
                ("sidebar_collapsed", _) => "devices;".into(),
                ("hidden_props_tabs", _) => "notes;".into(),
                ("show_thumbnails" | "text_in_icons" | "count_items", _) => "never".into(),
                ("own_colors", _) => "#123456;".into(),
                _ if v.parse::<i32>().is_ok() => (v.parse::<i32>().unwrap() + 8).to_string(),
                _ if v.starts_with('[') => {
                    let mut items: Vec<String> = kf.string_list("General", k).unwrap().iter().map(|x| x.to_string()).collect();
                    if items.len() > 1 {
                        items.reverse();
                    } else {
                        items = vec!["changed".into()];
                    }
                    items.iter().map(|x| format!("{x};")).collect()
                }
                _ => "changed".into(),
            };
            kf.set_value("General", k, &new);
            let loaded = fields(&from_keyfile(&kf));
            let got = &loaded.iter().find(|(n, _)| n == k).unwrap().1;
            assert_ne!(got, v, "{k} isn't loaded (set to {new:?})");
        }
    }

    #[test]
    fn keybindings_round_trip() {
        let s = Settings {
            keybindings: vec![
                ("copy".into(), vec!["<Control>c".into(), "<Control>Insert".into()]),
                ("trash".into(), vec![]),
            ],
            ..Settings::default()
        };
        assert_eq!(from_keyfile(&to_keyfile(&s)).keybindings, s.keybindings);
    }

    #[test]
    fn odd_list_items_round_trip() {
        let odd: Vec<String> = ["a;b", "back\\slash", " leading", "trailing ", "two\nlines", "tab\there", "", "plain"]
            .map(String::from)
            .to_vec();
        let kf = glib::KeyFile::new();
        set_string_list(&kf, "G", "k", &odd);
        let back: Vec<String> = kf.string_list("G", "k").unwrap().iter().map(|x| x.to_string()).collect();
        assert_eq!(back, odd);
    }

    #[test]
    fn preview_settings() {
        let kf = glib::KeyFile::new();
        kf.set_value("General", "show_thumbnails", "never");
        kf.set_value("General", "text_in_icons", "bogus");
        kf.set_value("General", "thumbnail_limit", "1048576");
        let s = from_keyfile(&kf);
        assert_eq!((s.show_thumbnails.as_str(), s.text_in_icons.as_str(), s.count_items.as_str(), s.thumbnail_limit), ("never", "local", "local", 1 << 20));
        let d = Settings::default();
        let back = from_keyfile(&to_keyfile(&d));
        assert_eq!((back.show_thumbnails, back.thumbnail_limit), ("local".to_string(), 100 << 20));
        assert!(crate::util::preview_on("local", &gio::File::for_path("/home/x")));
        assert!(crate::util::preview_on("local", &gio::File::for_uri("trash:///x")));
        assert!(!crate::util::preview_on("local", &gio::File::for_uri("sftp://host/x")));
        assert!(crate::util::preview_on("always", &gio::File::for_uri("sftp://host/x")));
        assert!(!crate::util::preview_on("never", &gio::File::for_path("/home/x")));
    }

    #[test]
    fn columns_and_captions() {
        let kf = glib::KeyFile::new();
        kf.set_value("General", "list_columns", "owner;bogus;size;owner;orig;");
        kf.set_value("General", "icon_captions", "type;bogus;");
        let s = from_keyfile(&kf);
        assert_eq!(s.list_columns, ["owner", "size"]);
        assert_eq!(s.icon_captions, ["type", "none", "none"]);
        // No columns at all is a choice too, and kept.
        let none = Settings { list_columns: vec![], ..Settings::default() };
        assert!(from_keyfile(&to_keyfile(&none)).list_columns.is_empty());
        let d = Settings::default();
        assert_eq!(from_keyfile(&to_keyfile(&d)).icon_captions, ["none", "size", "mtime"]);
    }

    #[test]
    fn incomplete_and_invalid_values() {
        let kf = glib::KeyFile::new();
        let g = "General";
        // A toolbar order without the path, the view buttons and the info
        // button: the path goes first, the others after the item they follow
        // by default (split view, zoom). Unknown and repeated ids go.
        kf.set_value(g, "bar_order", "ops;zoom;bogus;split;zoom;");
        kf.set_value(g, "rename_tab_order", "regex;case;nope;case;");
        // Basic can't be hidden; unknown and repeated tabs go.
        kf.set_value(g, "hidden_props_tabs", "notes;basic;nope;notes;media;");
        kf.set_value(g, "sidebar_sections", "bookmarks;");
        kf.set_value(g, "sidebar_collapsed", "devices;network;");
        kf.set_integer(g, "sidebar_row_height", 999);
        kf.set_integer(g, "info_panel_width", 5);
        kf.set_value(g, "list_levels", "big;");
        kf.set_integer(g, "list_size", 99);
        let s = from_keyfile(&kf);
        assert_eq!(s.bar_order, ["path", "ops", "zoom", "info", "split", "view"]);
        assert_eq!(s.rename_tab_order, ["regex", "case", "insert", "remove", "replace", "enumerate"]);
        assert_eq!(s.hidden_props_tabs, ["notes", "media"]);
        assert_eq!(s.sidebar_sections, ["bookmarks", "computer", "devices"]);
        assert_eq!(s.sidebar_collapsed, ["devices"]);
        assert_eq!(s.sidebar_row_height, 200);
        assert_eq!(s.info_panel_width, 120);
        // Invalid levels are the default ones; a size beyond them is the
        // largest.
        assert_eq!(s.list_levels, LIST_SIZES);
        assert_eq!(s.list_size, 64);
        // A size that isn't one of the levels snaps to the nearest.
        kf.set_integer(g, "icon_size", 50);
        assert_eq!(from_keyfile(&kf).icon_size, 48);
    }

    #[test]
    fn random_values_load_into_valid_settings() {
        let def = Settings::default();
        let keys: Vec<String> = fields(&def).into_iter().map(|(k, _)| k).collect();
        let level_chars: Vec<char> = "0123456789, ;px-".chars().collect();
        crate::testutil::cases(2000, |r| {
            let text = r.text_of(&level_chars, 20);
            if let Some(v) = parse_levels(&text) {
                assert!(v.windows(2).all(|w| w[0] < w[1]) && v.iter().all(|l| (MIN_LEVEL..=MAX_LEVEL).contains(l)), "{text:?} → {v:?}");
                assert_eq!(parse_levels(&format_levels(&v)), Some(v.clone()));
                let size = r.int(-100, 700) as i32;
                assert!(v.contains(&snap(&v, size)));
                let s = step(&v, size, if r.chance(2) { 1 } else { -1 });
                assert!(v.contains(&s) || s == size);
            }
            // Any keys with any values: still a valid configuration.
            let kf = to_keyfile(&def);
            for _ in 0..r.below(6) {
                let value = if r.chance(2) { r.int(-1000, 1000).to_string() } else { r.text(12) };
                // A line in a text file: no line break, no NUL.
                kf.set_value("General", r.pick(&keys), &value.replace('\n', " ").replace('\0', ""));
            }
            let s = from_keyfile(&kf);
            assert!(s.icon_levels.contains(&s.icon_size) && s.list_levels.contains(&s.list_size) && s.compact_levels.contains(&s.compact_size));
            let mut bar = s.bar_order.clone();
            bar.sort();
            let mut all: Vec<String> = BAR_ITEMS.iter().map(|x| x.to_string()).collect();
            all.sort();
            assert_eq!(bar, all);
            let mut secs = s.sidebar_sections.clone();
            secs.sort();
            assert_eq!(secs, ["bookmarks", "computer", "devices"]);
            assert_eq!(s.rename_tab_order.len(), RENAME_TABS.len());
        });
    }
}
