//! Keyboard shortcuts: every action that can have one, its defaults, and the
//! user's changes (`Settings::keybindings`), applied as GTK accelerators.

use gtk::prelude::*;

use crate::config;
use crate::i18n::n_;

pub struct Binding {
    /// Detailed action name, e.g. "win.copy" or "win.view-mode::list".
    pub action: &'static str,
    /// The label and group, translated where shown (tr()).
    pub label: &'static str,
    pub group: &'static str,
    pub defaults: &'static [&'static str],
}

const fn b(group: &'static str, action: &'static str, label: &'static str, defaults: &'static [&'static str]) -> Binding {
    Binding { action, label, group, defaults }
}

pub const BINDINGS: &[Binding] = &[
    b(n_("Windows and Tabs"), "win.new-tab", n_("New tab"), &["<Ctrl>t"]),
    b(n_("Windows and Tabs"), "win.new-window", n_("New window"), &["<Ctrl>n"]),
    b(n_("Windows and Tabs"), "win.close-tab", n_("Close tab"), &["<Ctrl>w"]),
    b(n_("Windows and Tabs"), "win.reopen-tab", n_("Reopen closed tab"), &["<Ctrl><Shift>t"]),
    b(n_("Windows and Tabs"), "win.close-window", n_("Close window"), &["<Ctrl><Shift>w"]),
    b(n_("Windows and Tabs"), "app.quit", n_("Quit"), &["<Ctrl>q"]),
    b(n_("Windows and Tabs"), "win.close-all-windows", n_("Close all windows"), &[]),
    b(n_("Windows and Tabs"), "win.tab-left", n_("Move tab left"), &["<Ctrl><Shift>Page_Up"]),
    b(n_("Windows and Tabs"), "win.tab-right", n_("Move tab right"), &["<Ctrl><Shift>Page_Down"]),
    b(n_("Windows and Tabs"), "win.switch-pane", n_("Switch to the other pane (split view)"), &["F6"]),
    b(n_("Windows and Tabs"), "win.same-location", n_("Same location as the other pane"), &[]),
    b(n_("Windows and Tabs"), "win.next-tab", n_("Next tab"), &["<Ctrl>Page_Down", "<Ctrl>Tab"]),
    b(n_("Windows and Tabs"), "win.prev-tab", n_("Previous tab"), &["<Ctrl>Page_Up", "<Ctrl><Shift>Tab"]),
    b(n_("Windows and Tabs"), "win.tab-1", n_("Go to tab 1"), &["<Alt>1"]),
    b(n_("Windows and Tabs"), "win.tab-2", n_("Go to tab 2"), &["<Alt>2"]),
    b(n_("Windows and Tabs"), "win.tab-3", n_("Go to tab 3"), &["<Alt>3"]),
    b(n_("Windows and Tabs"), "win.tab-4", n_("Go to tab 4"), &["<Alt>4"]),
    b(n_("Windows and Tabs"), "win.tab-5", n_("Go to tab 5"), &["<Alt>5"]),
    b(n_("Windows and Tabs"), "win.tab-6", n_("Go to tab 6"), &["<Alt>6"]),
    b(n_("Windows and Tabs"), "win.tab-7", n_("Go to tab 7"), &["<Alt>7"]),
    b(n_("Windows and Tabs"), "win.tab-8", n_("Go to tab 8"), &["<Alt>8"]),
    b(n_("Windows and Tabs"), "win.tab-9", n_("Go to last tab"), &["<Alt>9"]),
    b(n_("Navigation"), "win.back", n_("Back"), &["<Alt>Left", "XF86Back"]),
    b(n_("Navigation"), "win.forward", n_("Forward"), &["<Alt>Right", "XF86Forward"]),
    b(n_("Navigation"), "win.up", n_("Open parent folder"), &["<Alt>Up", "BackSpace"]),
    b(n_("Navigation"), "win.home", n_("Home folder"), &["<Alt>Home"]),
    b(n_("Navigation"), "win.go-root", n_("File system"), &[]),
    b(n_("Navigation"), "win.go-desktop", n_("Desktop"), &[]),
    b(n_("Navigation"), "win.go-trash", n_("Trash"), &[]),
    b(n_("Navigation"), "win.location", n_("Type a location"), &["<Ctrl>l"]),
    b(n_("Navigation"), "win.filter", n_("Search for files"), &["<Ctrl>f"]),
    b(n_("Navigation"), "win.reload", n_("Reload"), &["F5", "<Ctrl>r"]),
    b(n_("Navigation"), "win.stop", n_("Stop loading"), &[]),
    b(n_("Navigation"), "win.clear-history", n_("Clear history (back and forward)"), &[]),
    b(n_("Navigation"), "win.escape", n_("Cancel (close search or location bar)"), &["Escape"]),
    b(n_("Opening"), "win.open", n_("Open"), &["<Ctrl>o", "<Alt>Down"]),
    b(n_("Opening"), "win.open-new-tab", n_("Open in new tab"), &["<Ctrl>Return"]),
    b(n_("Navigation"), "win.connect-server", n_("Connect to server"), &[]),
    b(n_("Opening"), "win.open-new-window", n_("Open in new window"), &[]),
    b(n_("Opening"), "win.open-with-other", n_("Open with other application"), &[]),
    b(n_("Opening"), "win.terminal", n_("Open in terminal"), &["F4"]),
    b(n_("Files"), "win.new-folder", n_("New folder"), &["<Ctrl><Shift>n"]),
    b(n_("Files"), "win.new-file", n_("New empty document"), &[]),
    b(n_("Files"), "win.cut", n_("Cut"), &["<Ctrl>x"]),
    b(n_("Files"), "win.copy", n_("Copy"), &["<Ctrl>c"]),
    b(n_("Files"), "win.paste", n_("Paste"), &["<Ctrl>v"]),
    b(n_("Files"), "win.paste-check", n_("Paste and check the copies"), &["<Ctrl><Shift>v"]),
    b(n_("Files"), "win.paste-into", n_("Paste into folder"), &[]),
    b(n_("Files"), "win.undo", n_("Undo the last file operation"), &["<Ctrl>z"]),
    b(n_("Files"), "win.redo", n_("Redo the last undone file operation"), &["<Ctrl><Shift>z", "<Ctrl>y"]),
    b(n_("Files"), "win.rename", n_("Rename"), &["F2"]),
    b(n_("Files"), "win.rename-all", n_("Rename All… / Advanced Rename…"), &[]),
    b(n_("Files"), "win.duplicate", n_("Duplicate"), &[]),
    b(n_("Files"), "win.make-link", n_("Make link"), &["<Ctrl>m"]),
    b(n_("Files"), "win.copy-to", n_("Copy to…"), &[]),
    b(n_("Files"), "win.move-to", n_("Move to…"), &[]),
    b(n_("Files"), "win.copy-to-pane", n_("Copy to other pane"), &[]),
    b(n_("Files"), "win.move-to-pane", n_("Move to other pane"), &[]),
    b(n_("Files"), "win.trash", n_("Move to Trash"), &["Delete", "KP_Delete"]),
    b(n_("Files"), "win.delete", n_("Delete permanently"), &["<Shift>Delete", "<Shift>KP_Delete"]),
    b(n_("Files"), "win.restore", n_("Restore from Trash"), &[]),
    b(n_("Files"), "win.empty-trash", n_("Empty Trash"), &[]),
    b(n_("Files"), "win.extract", n_("Extract here"), &[]),
    b(n_("Files"), "win.compress", n_("Compress…"), &[]),
    b(n_("Files"), "win.properties", n_("Properties"), &["<Alt>Return", "<Ctrl>i"]),
    b(n_("Selection"), "win.select-all", n_("Select all"), &["<Ctrl>a"]),
    b(n_("Selection"), "win.invert-selection", n_("Invert selection"), &["<Ctrl><Shift>i"]),
    b(n_("Selection"), "win.select-pattern", n_("Select by pattern"), &["<Ctrl>s"]),
    b(n_("View"), "win.view-mode::icon", n_("Icon view"), &["<Ctrl>1"]),
    b(n_("View"), "win.view-mode::list", n_("List view"), &["<Ctrl>2"]),
    b(n_("View"), "win.view-mode::compact", n_("Compact view"), &["<Ctrl>3"]),
    b(n_("View"), "win.view-mode::columns", n_("Columns view"), &["<Ctrl>4"]),
    b(n_("View"), "win.show-hidden", n_("Show hidden files"), &["<Ctrl>h"]),
    b(n_("View"), "win.show-sidebar", n_("Side pane"), &["F9"]),
    b(n_("View"), "win.split-view", n_("Extra pane (split view)"), &["F3"]),
    b(n_("View"), "win.swap-panes", n_("Swap the panes (split view)"), &["<Ctrl>u"]),
    b(n_("View"), "win.focus-follows-mouse", n_("Focus the pane under the mouse pointer"), &["<Shift>F3"]),
    b(n_("View"), "win.info-panel", n_("Info panel (for this folder)"), &["F11"]),
    b(n_("View"), "win.show-statusbar", n_("Statusbar"), &[]),
    b(n_("View"), "win.show-toolbar", n_("Main toolbar"), &[]),
    b(n_("View"), "win.show-locbar", n_("Location bar"), &[]),
    b(n_("View"), "win.reset-view", n_("Reset view to defaults"), &[]),
    b(n_("View"), "win.lock-bar", n_("Lock the toolbar (no dragging its buttons around)"), &[]),
    b(n_("View"), "win.zoom-in", n_("Zoom in"), &["<Ctrl>plus", "<Ctrl>equal", "<Ctrl>KP_Add"]),
    b(n_("View"), "win.zoom-out", n_("Zoom out"), &["<Ctrl>minus", "<Ctrl>KP_Subtract"]),
    b(n_("View"), "win.zoom-normal", n_("Normal size"), &["<Ctrl>0", "<Ctrl>KP_0"]),
    b(n_("View"), "win.sort-by::name", n_("Sort by name"), &[]),
    b(n_("View"), "win.sort-by::size", n_("Sort by size"), &[]),
    b(n_("View"), "win.sort-by::type", n_("Sort by type"), &[]),
    b(n_("View"), "win.sort-by::mtime", n_("Sort by modification date"), &[]),
    b(n_("View"), "win.sort-desc", n_("Reversed order"), &[]),
    b(n_("View"), "win.folders-first", n_("Folders before files"), &[]),
    b(n_("Bookmarks and Help"), "win.add-bookmark", n_("Add bookmark"), &["<Ctrl>d"]),
    b(n_("Bookmarks and Help"), "win.edit-bookmarks", n_("Edit bookmarks"), &["<Ctrl>b"]),
    b(n_("Bookmarks and Help"), "win.preferences", n_("Preferences"), &[]),
    b(n_("Bookmarks and Help"), "win.manual", n_("Manual"), &["F1"]),
    b(n_("Bookmarks and Help"), "win.shortcuts", n_("Keyboard shortcuts"), &["<Ctrl>question", "<Ctrl>F1"]),
    b(n_("Bookmarks and Help"), "win.about", n_("About"), &[]),
];

/// The shortcuts `action` has now: the user's, or the defaults.
pub fn current(action: &str) -> Vec<String> {
    current_in(&config::get(), action)
}

fn current_in(s: &config::Settings, action: &str) -> Vec<String> {
    match s.keybindings.iter().find(|(a, _)| a == action) {
        Some((_, accels)) => accels.clone(),
        None => BINDINGS
            .iter()
            .find(|b| b.action == action)
            .map(|b| b.defaults.iter().map(|d| d.to_string()).collect())
            .unwrap_or_default(),
    }
}

pub fn is_default(action: &str) -> bool {
    !config::get().keybindings.iter().any(|(a, _)| a == action)
}

/// Sets `action`'s shortcuts (None = back to the defaults) and applies them.
pub fn set(app: &gtk::Application, action: &str, accels: Option<Vec<String>>) {
    config::update(|s| set_in(s, action, accels));
    apply(app);
}

/// Keeps `accels` for `action` in `s` unless they're its defaults.
fn set_in(s: &mut config::Settings, action: &str, accels: Option<Vec<String>>) {
    s.keybindings.retain(|(a, _)| a != action);
    if let Some(accels) = accels {
        let default = BINDINGS.iter().find(|b| b.action == action).map(|b| b.defaults.to_vec()).unwrap_or_default();
        if accels != default {
            s.keybindings.push((action.to_string(), accels));
        }
    }
}

pub fn reset_all(app: &gtk::Application) {
    config::update(|s| s.keybindings.clear());
    apply(app);
}

pub fn apply(app: &gtk::Application) {
    let s = config::get();
    for b in BINDINGS {
        let accels = current_in(&s, b.action);
        app.set_accels_for_action(b.action, &accels.iter().map(String::as_str).collect::<Vec<_>>());
    }
}

/// The other action that already uses `accel`, if any.
pub fn used_by(accel: &str, except: &str) -> Option<&'static Binding> {
    let s = config::get();
    let want = gtk::accelerator_parse(accel);
    BINDINGS
        .iter()
        .filter(|b| b.action != except)
        .find(|b| current_in(&s, b.action).iter().any(|a| gtk::accelerator_parse(a) == want))
}

/// "Ctrl+Shift+N, F5" for showing a list of shortcuts.
pub fn labels(accels: &[String]) -> String {
    accels
        .iter()
        .map(|a| {
            let (key, mods) = gtk::accelerator_parse(a);
            gtk::accelerator_get_label(key, mods).map(|l| l.to_string()).unwrap_or_else(|| a.clone())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The manual's list of the usual shortcuts (in
/// docs/manual/en/shortcuts.md, between its markers): a table for each group,
/// in Markdown, the keys written out as on a keyboard. Checked against
/// BINDINGS by a test, which writes it with ARKHA_WRITE_MANUAL=1.
#[cfg(test)]
fn manual_table() -> String {
    let key = |name: &str| -> String {
        match name {
            "plus" => "+".into(),
            "equal" => "=".into(),
            "minus" => "-".into(),
            "question" => "?".into(),
            "BackSpace" => "Backspace".into(),
            "Return" => "Enter".into(),
            "Left" => "←".into(),
            "Right" => "→".into(),
            "Up" => "↑".into(),
            "Down" => "↓".into(),
            "XF86Back" => "Back".into(),
            "XF86Forward" => "Forward".into(),
            n if n.chars().count() == 1 => n.to_uppercase(),
            n => n.replace('_', " "),
        }
    };
    let keys = |accel: &str| -> String {
        let mut rest = accel;
        let mut parts = vec![];
        while let Some(end) = rest.strip_prefix('<').and_then(|r| r.find('>')) {
            parts.push(rest[1..end + 1].to_string());
            rest = &rest[end + 2..];
        }
        parts.push(key(rest));
        parts.iter().map(|p| format!("<kbd>{p}</kbd>")).collect::<Vec<_>>().join("+")
    };
    let mut groups: Vec<&str> = vec![];
    for b in BINDINGS {
        if !groups.contains(&b.group) {
            groups.push(b.group);
        }
    }
    let mut out = String::new();
    for g in groups {
        out.push_str(&format!("\n## {g}\n\n| Action | Keys |\n|---|---|\n"));
        for b in BINDINGS.iter().filter(|b| b.group == g && !b.defaults.is_empty()) {
            // The keypad's keys are the same as the others.
            let accels: Vec<String> = b.defaults.iter().filter(|a| !a.contains("KP_")).map(|a| keys(a)).collect();
            out.push_str(&format!("| {} | {} |\n", b.label, accels.join(", ")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_lists_the_usual_shortcuts() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/manual/en/shortcuts.md");
        let text = std::fs::read_to_string(&path).unwrap();
        let (start, end) = ("<!-- The tables below are made from src/keys.rs. -->\n", "\n<!-- End of the tables. -->");
        let (before, rest) = text.split_once(start).expect("the start marker");
        let (table, after) = rest.split_once(end).expect("the end marker");
        let want = manual_table();
        if std::env::var_os("ARKHA_WRITE_MANUAL").is_some() {
            std::fs::write(&path, format!("{before}{start}{want}{end}{after}")).unwrap();
            return;
        }
        assert!(table == want, "{} doesn't list the shortcuts as they are: ARKHA_WRITE_MANUAL=1 cargo test manual_lists writes them", path.display());
    }

    #[test]
    fn user_shortcuts_replace_the_defaults() {
        let mut s = config::Settings::default();
        assert_eq!(current_in(&s, "win.new-folder"), ["<Ctrl><Shift>n"]);
        s.keybindings = vec![("win.new-folder".into(), vec!["F7".into()]), ("win.about".into(), vec![])];
        assert_eq!(current_in(&s, "win.new-folder"), ["F7"]);
        assert!(current_in(&s, "win.about").is_empty());
        assert!(current_in(&s, "win.no-such-action").is_empty());
    }

    #[test]
    fn shortcuts_kept_only_when_not_the_defaults() {
        let mut s = config::Settings::default();
        set_in(&mut s, "win.new-folder", Some(vec!["<Ctrl><Shift>n".into()]));
        assert!(s.keybindings.is_empty());
        set_in(&mut s, "win.new-folder", Some(vec!["F7".into()]));
        set_in(&mut s, "win.reload", Some(vec![]));
        assert_eq!(s.keybindings, [("win.new-folder".to_string(), vec!["F7".to_string()]), ("win.reload".to_string(), vec![])]);
        assert_eq!(current_in(&s, "win.new-folder"), ["F7"]);
        // None: back to the defaults.
        set_in(&mut s, "win.new-folder", None);
        assert_eq!(s.keybindings, [("win.reload".to_string(), vec![])]);
        assert_eq!(current_in(&s, "win.new-folder"), ["<Ctrl><Shift>n"]);
    }

    #[test]
    fn catalogue_is_consistent() {
        let mut actions: Vec<&str> = BINDINGS.iter().map(|b| b.action).collect();
        actions.sort_unstable();
        let n = actions.len();
        actions.dedup();
        assert_eq!(actions.len(), n, "an action is listed twice");
        let mut accels: Vec<&str> = BINDINGS.iter().flat_map(|b| b.defaults.iter().copied()).collect();
        accels.sort_unstable();
        let n = accels.len();
        accels.dedup();
        assert_eq!(accels.len(), n, "two actions share a default shortcut");
    }
}
