//! Persistent settings in ~/.config/arkha/arkha.conf

use std::cell::RefCell;

#[derive(Clone, Debug)]
pub struct Settings {
    pub show_hidden: bool,
    pub default_view: String, // "icon" | "list" | "compact"
    /// Current zoom of each view in pixels; always one of its zoom levels.
    pub icon_size: i32,
    pub list_size: i32,
    pub compact_size: i32,
    /// The sizes Zoom In / Zoom Out step through, ascending.
    pub icon_levels: Vec<i32>,
    pub list_levels: Vec<i32>,
    pub compact_levels: Vec<i32>,
    pub sort_by: String,      // "name" | "size" | "type" | "mtime"
    pub sort_desc: bool,
    pub folders_first: bool,
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
    pub sidebar_width: i32,
    pub show_sidebar: bool,
    pub show_statusbar: bool,
    pub show_toolbar: bool,
    /// The mouse's extra buttons go back and forward. As in Caja (whose
    /// dconf keys these are), only in the settings file, not in Preferences.
    pub mouse_extra_buttons: bool,
    pub mouse_back_button: i32,
    pub mouse_forward_button: i32,
    /// Icons beside the names in the list view.
    pub list_icons: bool,
    /// The location bar (the path, the bar beside it). Hidden, it still
    /// shows while the location field or the search is open.
    pub show_locbar: bool,
    /// The bar beside the path can't be rearranged: no accidental drags
    /// (on a touchpad, say).
    pub lock_bar: bool,
    /// The status bar says how many items are hidden.
    pub status_hidden: bool,
    /// Show the tab bar even with one tab (so that tab can be dragged).
    pub always_show_tabs: bool,
    /// Text next to the icons of toolbar buttons (Back, Forward).
    pub toolbar_text: bool,
    /// Shell that runs custom context menu actions, as `<shell> -c <command>`.
    pub action_shell: String,
    /// The archive manager for Extract Here and Compress (its command, see
    /// util::ARCHIVERS), or "" for the one set for archives (see
    /// util::archiver_command).
    pub archiver: String,
    /// In split view, the pane under the mouse pointer gets focus.
    pub focus_follows_mouse: bool,
    /// How long (ms) the pointer has to stay over a pane before it takes the
    /// focus: passing over it on the way elsewhere doesn't count.
    pub focus_follows_mouse_delay: i32,
    /// Which pane stays when split view is turned off: "active" (the one
    /// in use) or "other".
    pub split_keep: String,
    /// Where the extra pane opens: "same" (the folder of the pane in use),
    /// "home", "last" (where the closed pane was, `split_last`) or
    /// "folder" (`split_folder`).
    pub split_opens: String,
    /// The folder (a URI) for split_opens "folder".
    pub split_folder: String,
    /// The folder (a URI) the pane that closed with split view last showed.
    pub split_last: String,
    /// Where New Tab opens, as split_opens: "same" (the current tab's
    /// folder), "home", "last" (`tab_last`) or "folder" (`tab_folder`).
    pub tab_opens: String,
    pub tab_folder: String,
    /// The folder (a URI) of the tab closed last.
    pub tab_last: String,
    /// New tabs go at the end of the tab bar (else right after the current one).
    /// Where new tabs go: "in-order" (as Firefox puts them), "after" (right
    /// after the current tab) or "end".
    pub new_tabs: String,
    /// Button groups at the right end of the location bar.
    pub show_zoom_buttons: bool,
    pub show_view_buttons: bool,
    pub show_split_button: bool,
    /// The Swap Panes button beside it (shown in split view).
    pub show_swap_button: bool,
    pub show_info_button: bool,
    /// Order of the location bar items (see BAR_ITEMS); rearranged by dragging.
    pub bar_order: Vec<String>,
    /// Order of the bulk rename window's tabs (see RENAME_TABS); rearranged by dragging.
    pub rename_tab_order: Vec<String>,
    /// Order of the Properties window's tabs (see PROPS_TABS), the same way.
    pub props_tab_order: Vec<String>,
    /// The Properties tabs never shown (Preferences → Properties); never
    /// "basic", so the window always has a tab.
    pub hidden_props_tabs: Vec<String>,
    /// The checksums the Checksums tab offers: all of them in their order
    /// (ids as checksums::Algo::id), and those not offered.
    pub checksum_order: Vec<String>,
    pub hidden_checksums: Vec<String>,
    /// Order of the side pane's sections (see SIDEBAR_SECTIONS); rearranged
    /// by dragging their headings.
    pub sidebar_sections: Vec<String>,
    /// The list view's columns after the name, in order (see details.rs).
    pub list_columns: Vec<String>,
    /// What shows under the names in the icon view, the more of them the
    /// bigger the icons (see details::shown_captions); "none" leaves a gap.
    pub icon_captions: Vec<String>,
    /// Side pane sections showing only their heading.
    pub sidebar_collapsed: Vec<String>,
    /// Mount removable media when inserted (see sidebar::init_automount);
    /// only used without a desktop, whose own setting this is otherwise.
    pub automount: bool,
    /// The side pane's eject button only unmounts (no eject / power off).
    pub unmount_only: bool,
    /// Separate automounters (udiskie, …) already told about, once each.
    pub automounters_noticed: Vec<String>,
    /// What happens with media Arkha mounts when inserted (see autorun.rs),
    /// as the desktop's media-handling keys say, which are these otherwise
    /// (without one of GNOME, Cinnamon or MATE).
    pub autorun_never: bool,
    pub automount_open: bool,
    pub autorun_start_app: Vec<String>,
    pub autorun_ignore: Vec<String>,
    pub autorun_open_folder: Vec<String>,
    /// Always show the location as editable text instead of breadcrumbs.
    pub path_as_text: bool,
    pub confirm_trash: bool,
    /// File operations in their own window (opened when one takes a while)
    /// instead of the location bar's button and its popover.
    pub ops_window: bool,
    /// A new search looks in the subfolders too (each search can change it).
    pub search_subfolders: bool,
    pub single_click: bool,
    /// Previews, as Caja has them, each "always", "local" (local files
    /// only: not GVfs places such as network shares) or "never" (see
    /// util::preview_on): thumbnails, a text file's first lines in its icon,
    /// and how many items a folder has.
    pub show_thumbnails: String,
    /// Colours added in Backgrounds and Emblems ("#rrggbb").
    pub own_colors: Vec<String>,
    /// The side pane's background: a colour ("#rrggbb") and/or a picture's
    /// URI (see backgrounds.rs); empty for the theme's.
    pub side_pane_color: String,
    pub side_pane_image: String,
    pub text_in_icons: String,
    pub count_items: String,
    /// Images bigger than this (bytes) get no thumbnail (other files'
    /// thumbnailers decide for themselves), as in Caja.
    pub thumbnail_limit: u64,
    /// Side pane entries the user hid, by URI (or "label:Name" for unmounted drives).
    pub hidden_places: Vec<String>,
    /// The Computer entries' order in the side pane, by the same keys
    /// (empty: the usual order; see sidebar::ordered_places).
    pub places_order: Vec<String>,
    /// Built-in context menu items the user hid (keys from actions.rs).
    pub hidden_menu_items: Vec<String>,
    /// Order of the context menu for selected items and for a folder's
    /// empty space (see actions::menu_order); empty = the default.
    pub menu_order: Vec<String>,
    pub menu_order_bg: Vec<String>,
    /// Height of side pane rows in pixels; 0 = whatever the theme uses.
    pub sidebar_row_height: i32,
    /// Remember view and sorting per folder (see folders.rs).
    pub per_folder: bool,
    /// Alternating row colors in the list view.
    pub striped_rows: bool,
    /// The same in the bulk rename window's file list.
    pub rename_striped_rows: bool,
    /// File sizes in IEC units (KiB, MiB) instead of SI (kB, MB); see
    /// util::format_size.
    pub iec_units: bool,
    /// How dates are shown: "informal" (Today 14:00), "locale" or "iso".
    pub date_format: String,
    /// The language of Arkha's texts (a translation's code, "en"), or ""
    /// for the system's; applies when Arkha starts (see i18n).
    pub language: String,
    /// Folder covers (see info.rs): the file names looked for, ';'-separated
    /// and in order of preference, matched ignoring case.
    pub cover_names: String,
    /// What the info panel shows while several items are selected:
    /// "summary" (their number and total size), "last" (the last item),
    /// "current" (the open folder) or "hide" (the panel hides).
    pub info_several: String,
    /// For a selected folder without a cover: "icon" (a folder icon and
    /// the details) or "none" (nothing).
    pub info_no_cover: String,
    pub info_details: bool,
    /// The info panel when folders don't remember their own settings.
    pub info_panel: bool,
    pub info_panel_width: i32,
    /// Middle-click on empty space (and on files in the list view) starts
    /// autoscrolling (see scroll.rs).
    pub autoscroll: bool,
    /// "Media Info" tab (mediainfo output) in the properties dialog, and the
    /// kinds of files it's shown for.
    pub media_info: bool,
    pub media_info_video: bool,
    pub media_info_audio: bool,
    pub media_info_image: bool,
    pub media_info_other: bool,
    /// Shortcuts the user changed: (action, accelerators); an empty list
    /// means no shortcut. Actions not listed use their defaults (keys.rs).
    pub keybindings: Vec<(String, Vec<String>)>,
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

impl Default for Settings {
    fn default() -> Self {
        Settings {
            show_hidden: false,
            default_view: "icon".into(),
            icon_size: NORMAL_ICON,
            list_size: NORMAL_LIST,
            compact_size: NORMAL_COMPACT,
            icon_levels: ICON_SIZES.to_vec(),
            list_levels: LIST_SIZES.to_vec(),
            compact_levels: COMPACT_SIZES.to_vec(),
            sort_by: "name".into(),
            sort_desc: false,
            folders_first: true,
            width: 1000,
            height: 650,
            maximized: false,
            sidebar_width: 190,
            show_sidebar: true,
            show_statusbar: true,
            show_toolbar: true,
            mouse_extra_buttons: true,
            mouse_back_button: 8,
            mouse_forward_button: 9,
            list_icons: true,
            show_locbar: true,
            lock_bar: true,
            status_hidden: false,
            always_show_tabs: false,
            toolbar_text: false,
            action_shell: "sh".into(),
            archiver: String::new(),
            focus_follows_mouse: false,
            focus_follows_mouse_delay: 300,
            split_keep: "active".into(),
            split_opens: "same".into(),
            split_folder: String::new(),
            split_last: String::new(),
            tab_opens: "same".into(),
            tab_folder: String::new(),
            tab_last: String::new(),
            new_tabs: "in-order".into(),
            show_zoom_buttons: true,
            show_view_buttons: true,
            show_split_button: true,
            show_swap_button: true,
            show_info_button: true,
            bar_order: BAR_ITEMS.iter().map(|g| g.to_string()).collect(),
            rename_tab_order: RENAME_TABS.iter().map(|t| t.to_string()).collect(),
            props_tab_order: PROPS_TABS.iter().map(|t| t.to_string()).collect(),
            hidden_props_tabs: vec![],
            checksum_order: crate::checksums::ALL.iter().map(|a| a.id().to_string()).collect(),
            hidden_checksums: crate::checksums::HIDDEN_AT_FIRST.iter().map(|t| t.to_string()).collect(),
            sidebar_sections: SIDEBAR_SECTIONS.iter().map(|t| t.to_string()).collect(),
            list_columns: crate::details::DEFAULT_COLUMNS.iter().map(|t| t.to_string()).collect(),
            icon_captions: crate::details::DEFAULT_CAPTIONS.iter().map(|t| t.to_string()).collect(),
            sidebar_collapsed: vec![],
            automount: true,
            unmount_only: false,
            automounters_noticed: vec![],
            autorun_never: false,
            automount_open: true,
            autorun_start_app: vec![],
            autorun_ignore: vec![],
            autorun_open_folder: vec![],
            path_as_text: false,
            confirm_trash: true,
            ops_window: false,
            search_subfolders: true,
            single_click: false,
            show_thumbnails: "local".into(),
            own_colors: vec![],
            side_pane_color: String::new(),
            side_pane_image: String::new(),
            text_in_icons: "local".into(),
            count_items: "local".into(),
            thumbnail_limit: 100 << 20,
            hidden_places: vec![],
            places_order: vec![],
            hidden_menu_items: vec![],
            menu_order: vec![],
            menu_order_bg: vec![],
            sidebar_row_height: 0,
            per_folder: true,
            striped_rows: false,
            rename_striped_rows: true,
            iec_units: false,
            date_format: "informal".into(),
            language: String::new(),
            cover_names: "folder.jpg; folder.png; cover.jpg; cover.png; front.jpg".into(),
            info_several: "summary".into(),
            info_no_cover: "icon".into(),
            info_details: true,
            info_panel: false,
            info_panel_width: 240,
            autoscroll: true,
            media_info: true,
            media_info_video: true,
            media_info_audio: true,
            media_info_image: true,
            media_info_other: false,
            keybindings: vec![],
        }
    }
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
    let g = "General";
    macro_rules! b {
        ($f:ident) => {
            if let Ok(v) = kf.boolean(g, stringify!($f)) {
                s.$f = v;
            }
        };
    }
    macro_rules! i {
        ($f:ident) => {
            if let Ok(v) = kf.integer(g, stringify!($f)) {
                s.$f = v;
            }
        };
    }
    macro_rules! st {
        ($f:ident) => {
            if let Ok(v) = kf.string(g, stringify!($f)) {
                s.$f = v.to_string();
            }
        };
    }
    b!(show_hidden);
    st!(default_view);
    macro_rules! levels {
        ($f:ident) => {
            if let Some(v) = kf.integer_list(g, stringify!($f)).ok().and_then(|v| parse_levels(&format_levels(&v))) {
                s.$f = v;
            }
        };
    }
    levels!(icon_levels);
    levels!(list_levels);
    levels!(compact_levels);
    i!(icon_size);
    i!(list_size);
    i!(compact_size);
    s.icon_size = snap(&s.icon_levels, s.icon_size);
    s.list_size = snap(&s.list_levels, s.list_size);
    s.compact_size = snap(&s.compact_levels, s.compact_size);
    st!(sort_by);
    b!(sort_desc);
    b!(folders_first);
    i!(width);
    i!(height);
    b!(maximized);
    i!(sidebar_width);
    b!(show_sidebar);
    b!(show_statusbar);
    b!(show_toolbar);
    b!(mouse_extra_buttons);
    i!(mouse_back_button);
    i!(mouse_forward_button);
    b!(list_icons);
    b!(show_locbar);
    b!(lock_bar);
    b!(status_hidden);
    b!(always_show_tabs);
    b!(toolbar_text);
    st!(action_shell);
    st!(archiver);
    b!(focus_follows_mouse);
    i!(focus_follows_mouse_delay);
    st!(split_keep);
    st!(split_opens);
    st!(split_folder);
    st!(split_last);
    st!(tab_opens);
    st!(tab_folder);
    st!(tab_last);
    st!(new_tabs);
    b!(show_zoom_buttons);
    b!(show_view_buttons);
    b!(show_split_button);
    b!(show_swap_button);
    b!(show_info_button);
    b!(path_as_text);
    // Known tabs once each; any missing (e.g. added later) go at the end.
    let tab_order = |v: Vec<String>, known: &[&str]| -> Vec<String> {
        let mut order: Vec<String> = vec![];
        for id in v.into_iter().chain(known.iter().map(|t| t.to_string())) {
            if known.contains(&id.as_str()) && !order.contains(&id) {
                order.push(id);
            }
        }
        order
    };
    if let Ok(v) = kf.string_list(g, "rename_tab_order") {
        s.rename_tab_order = tab_order(v.iter().map(|x| x.to_string()).collect(), &RENAME_TABS);
    }
    if let Ok(v) = kf.string_list(g, "props_tab_order") {
        s.props_tab_order = tab_order(v.iter().map(|x| x.to_string()).collect(), &PROPS_TABS);
    }
    if let Ok(v) = kf.string_list(g, "hidden_props_tabs") {
        let mut hidden: Vec<String> = vec![];
        for t in v.iter().map(|x| x.to_string()) {
            if t != "basic" && PROPS_TABS.contains(&t.as_str()) && !hidden.contains(&t) {
                hidden.push(t);
            }
        }
        s.hidden_props_tabs = hidden;
    }
    let checksum_ids: Vec<&str> = crate::checksums::ALL.iter().map(|a| a.id()).collect();
    if let Ok(v) = kf.string_list(g, "checksum_order") {
        s.checksum_order = tab_order(v.iter().map(|x| x.to_string()).collect(), &checksum_ids);
    }
    if let Ok(v) = kf.string_list(g, "hidden_checksums") {
        s.hidden_checksums = v.iter().map(|x| x.to_string()).filter(|id| checksum_ids.contains(&id.as_str())).collect();
    }
    if let Ok(v) = kf.string_list(g, "list_columns") {
        let v: Vec<String> = v.iter().map(|x| x.to_string()).collect();
        s.list_columns = crate::details::known(&v).iter().map(|t| t.to_string()).collect();
    }
    if let Ok(v) = kf.string_list(g, "icon_captions") {
        // Three places; any not known are gaps.
        let v: Vec<String> = v.iter().map(|x| x.to_string()).collect();
        s.icon_captions = (0..3)
            .map(|i| v.get(i).filter(|c| crate::details::known(std::slice::from_ref(c)).len() == 1).cloned().unwrap_or_else(|| "none".into()))
            .collect();
    }
    if let Ok(v) = kf.string_list(g, "sidebar_sections") {
        let mut order: Vec<String> = vec![];
        for id in v.iter().map(|x| x.to_string()).chain(SIDEBAR_SECTIONS.iter().map(|t| t.to_string())) {
            if SIDEBAR_SECTIONS.contains(&id.as_str()) && !order.contains(&id) {
                order.push(id);
            }
        }
        s.sidebar_sections = order;
    }
    b!(automount);
    b!(unmount_only);
    if let Ok(v) = kf.string_list(g, "bar_order") {
        // Known ids once each. Missing ones (items added since it was
        // saved) go after the one they follow by default, or first.
        let mut order: Vec<String> = vec![];
        for id in v.iter().map(|x| x.to_string()) {
            if BAR_ITEMS.contains(&id.as_str()) && !order.contains(&id) {
                order.push(id);
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
    }
    b!(confirm_trash);
    b!(ops_window);
    b!(search_subfolders);
    b!(single_click);
    for (key, field) in [("show_thumbnails", &mut s.show_thumbnails), ("text_in_icons", &mut s.text_in_icons), ("count_items", &mut s.count_items)] {
        if let Ok(v) = kf.string(g, key) {
            if ["always", "local", "never"].contains(&v.as_str()) {
                *field = v.to_string();
            }
        }
    }
    if let Ok(v) = kf.string_list(g, "own_colors") {
        s.own_colors = v.iter().map(|x| x.to_string()).filter(|c| gdk::RGBA::parse(c).is_ok()).collect();
    }
    st!(side_pane_color);
    st!(side_pane_image);
    if let Ok(v) = kf.uint64(g, "thumbnail_limit") {
        s.thumbnail_limit = v;
    }
    i!(sidebar_row_height);
    b!(per_folder);
    b!(striped_rows);
    b!(rename_striped_rows);
    b!(iec_units);
    st!(date_format);
    st!(language);
    st!(cover_names);
    st!(info_several);
    st!(info_no_cover);
    b!(info_details);
    b!(info_panel);
    i!(info_panel_width);
    b!(autoscroll);
    b!(media_info);
    b!(media_info_video);
    b!(media_info_audio);
    b!(media_info_image);
    b!(media_info_other);
    if let Ok(keys) = kf.keys("Keybindings") {
        for k in keys.iter() {
            let accels = kf.string_list("Keybindings", k).map(|v| v.iter().map(|a| a.to_string()).filter(|a| !a.is_empty()).collect()).unwrap_or_default();
            s.keybindings.push((k.to_string(), accels));
        }
    }
    if let Ok(v) = kf.string_list(g, "sidebar_collapsed") {
        s.sidebar_collapsed = v.iter().map(|x| x.to_string()).filter(|x| SIDEBAR_SECTIONS.contains(&x.as_str())).collect();
    }
    if let Ok(v) = kf.string_list(g, "automounters_noticed") {
        s.automounters_noticed = v.iter().map(|x| x.to_string()).collect();
    }
    b!(autorun_never);
    b!(automount_open);
    for (key, list) in [("autorun_start_app", &mut s.autorun_start_app), ("autorun_ignore", &mut s.autorun_ignore), ("autorun_open_folder", &mut s.autorun_open_folder)] {
        if let Ok(v) = kf.string_list(g, key) {
            *list = v.iter().map(|x| x.to_string()).collect();
        }
    }
    if let Ok(v) = kf.string_list(g, "hidden_places") {
        s.hidden_places = v.iter().map(|x| x.to_string()).collect();
    }
    if let Ok(v) = kf.string_list(g, "places_order") {
        s.places_order = v.iter().map(|x| x.to_string()).collect();
    }
    if let Ok(v) = kf.string_list(g, "menu_order") {
        s.menu_order = v.iter().map(|x| x.to_string()).collect();
    }
    if let Ok(v) = kf.string_list(g, "menu_order_bg") {
        s.menu_order_bg = v.iter().map(|x| x.to_string()).collect();
    }
    if let Ok(v) = kf.string_list(g, "hidden_menu_items") {
        s.hidden_menu_items = v.iter().map(|x| x.to_string()).collect();
    }
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
    let g = "General";
    kf.set_boolean(g, "show_hidden", s.show_hidden);
    kf.set_string(g, "default_view", &s.default_view);
    kf.set_integer(g, "icon_size", s.icon_size);
    kf.set_integer(g, "list_size", s.list_size);
    kf.set_integer(g, "compact_size", s.compact_size);
    for (key, levels) in [("icon_levels", &s.icon_levels), ("list_levels", &s.list_levels), ("compact_levels", &s.compact_levels)] {
        kf.set_value(g, key, &levels.iter().map(|l| format!("{l};")).collect::<String>());
    }
    kf.set_string(g, "sort_by", &s.sort_by);
    kf.set_boolean(g, "sort_desc", s.sort_desc);
    kf.set_boolean(g, "folders_first", s.folders_first);
    kf.set_integer(g, "width", s.width);
    kf.set_integer(g, "height", s.height);
    kf.set_boolean(g, "maximized", s.maximized);
    kf.set_integer(g, "sidebar_width", s.sidebar_width);
    kf.set_boolean(g, "show_sidebar", s.show_sidebar);
    kf.set_boolean(g, "show_statusbar", s.show_statusbar);
    kf.set_boolean(g, "show_toolbar", s.show_toolbar);
    kf.set_boolean(g, "mouse_extra_buttons", s.mouse_extra_buttons);
    kf.set_integer(g, "mouse_back_button", s.mouse_back_button);
    kf.set_integer(g, "mouse_forward_button", s.mouse_forward_button);
    kf.set_boolean(g, "list_icons", s.list_icons);
    kf.set_boolean(g, "show_locbar", s.show_locbar);
    kf.set_boolean(g, "lock_bar", s.lock_bar);
    kf.set_boolean(g, "status_hidden", s.status_hidden);
    kf.set_boolean(g, "always_show_tabs", s.always_show_tabs);
    kf.set_boolean(g, "toolbar_text", s.toolbar_text);
    kf.set_string(g, "action_shell", &s.action_shell);
    kf.set_string(g, "archiver", &s.archiver);
    kf.set_boolean(g, "focus_follows_mouse", s.focus_follows_mouse);
    kf.set_integer(g, "focus_follows_mouse_delay", s.focus_follows_mouse_delay);
    kf.set_string(g, "split_keep", &s.split_keep);
    kf.set_string(g, "split_opens", &s.split_opens);
    kf.set_string(g, "split_folder", &s.split_folder);
    kf.set_string(g, "split_last", &s.split_last);
    kf.set_string(g, "tab_opens", &s.tab_opens);
    kf.set_string(g, "tab_folder", &s.tab_folder);
    kf.set_string(g, "tab_last", &s.tab_last);
    kf.set_string(g, "new_tabs", &s.new_tabs);
    kf.set_boolean(g, "show_zoom_buttons", s.show_zoom_buttons);
    kf.set_boolean(g, "show_view_buttons", s.show_view_buttons);
    kf.set_boolean(g, "show_split_button", s.show_split_button);
    kf.set_boolean(g, "show_swap_button", s.show_swap_button);
    kf.set_boolean(g, "show_info_button", s.show_info_button);
    set_string_list(&kf, g, "bar_order", &s.bar_order);
    set_string_list(&kf, g, "rename_tab_order", &s.rename_tab_order);
    set_string_list(&kf, g, "props_tab_order", &s.props_tab_order);
    set_string_list(&kf, g, "hidden_props_tabs", &s.hidden_props_tabs);
    set_string_list(&kf, g, "checksum_order", &s.checksum_order);
    set_string_list(&kf, g, "hidden_checksums", &s.hidden_checksums);
    set_string_list(&kf, g, "sidebar_sections", &s.sidebar_sections);
    set_string_list(&kf, g, "list_columns", &s.list_columns);
    set_string_list(&kf, g, "icon_captions", &s.icon_captions);
    kf.set_boolean(g, "automount", s.automount);
    kf.set_boolean(g, "unmount_only", s.unmount_only);
    kf.set_boolean(g, "path_as_text", s.path_as_text);
    kf.set_boolean(g, "confirm_trash", s.confirm_trash);
    kf.set_boolean(g, "ops_window", s.ops_window);
    kf.set_boolean(g, "search_subfolders", s.search_subfolders);
    kf.set_boolean(g, "single_click", s.single_click);
    kf.set_string(g, "show_thumbnails", &s.show_thumbnails);
    set_string_list(&kf, g, "own_colors", &s.own_colors);
    kf.set_string(g, "side_pane_color", &s.side_pane_color);
    kf.set_string(g, "side_pane_image", &s.side_pane_image);
    kf.set_string(g, "text_in_icons", &s.text_in_icons);
    kf.set_string(g, "count_items", &s.count_items);
    kf.set_uint64(g, "thumbnail_limit", s.thumbnail_limit);
    kf.set_integer(g, "sidebar_row_height", s.sidebar_row_height);
    kf.set_boolean(g, "per_folder", s.per_folder);
    kf.set_boolean(g, "striped_rows", s.striped_rows);
    kf.set_boolean(g, "rename_striped_rows", s.rename_striped_rows);
    kf.set_boolean(g, "iec_units", s.iec_units);
    kf.set_string(g, "date_format", &s.date_format);
    kf.set_string(g, "language", &s.language);
    kf.set_string(g, "cover_names", &s.cover_names);
    kf.set_string(g, "info_several", &s.info_several);
    kf.set_string(g, "info_no_cover", &s.info_no_cover);
    kf.set_boolean(g, "info_details", s.info_details);
    kf.set_boolean(g, "info_panel", s.info_panel);
    kf.set_integer(g, "info_panel_width", s.info_panel_width);
    kf.set_boolean(g, "autoscroll", s.autoscroll);
    kf.set_boolean(g, "media_info", s.media_info);
    kf.set_boolean(g, "media_info_video", s.media_info_video);
    kf.set_boolean(g, "media_info_audio", s.media_info_audio);
    kf.set_boolean(g, "media_info_image", s.media_info_image);
    kf.set_boolean(g, "media_info_other", s.media_info_other);
    for (action, accels) in &s.keybindings {
        set_string_list(&kf, "Keybindings", action, accels);
    }
    set_string_list(&kf, g, "hidden_places", &s.hidden_places);
    set_string_list(&kf, g, "places_order", &s.places_order);
    set_string_list(&kf, g, "automounters_noticed", &s.automounters_noticed);
    kf.set_boolean(g, "autorun_never", s.autorun_never);
    kf.set_boolean(g, "automount_open", s.automount_open);
    set_string_list(&kf, g, "autorun_start_app", &s.autorun_start_app);
    set_string_list(&kf, g, "autorun_ignore", &s.autorun_ignore);
    set_string_list(&kf, g, "autorun_open_folder", &s.autorun_open_folder);
    set_string_list(&kf, g, "sidebar_collapsed", &s.sidebar_collapsed);
    set_string_list(&kf, g, "hidden_menu_items", &s.hidden_menu_items);
    set_string_list(&kf, g, "menu_order", &s.menu_order);
    set_string_list(&kf, g, "menu_order_bg", &s.menu_order_bg);
    kf
}

#[cfg(test)]
mod tests {
    use super::*;

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
