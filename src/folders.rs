//! View, sorting and the info panel remembered for individual folders, in
//! ~/.config/arkha/folders.conf (one group per folder URI). Used when the
//! "remember view and sorting for each folder" preference is on. Only what
//! differs from the defaults (Preferences) is kept: a setting set back to
//! its default goes, and so does a folder with nothing left.

use std::cell::RefCell;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderView {
    pub view: Option<String>,
    pub sort: Option<(String, bool)>,
    /// The info panel (see info.rs) shown in this folder.
    pub info_panel: Option<bool>,
}

thread_local! {
    static STORE: RefCell<Option<glib::KeyFile>> = const { RefCell::new(None) };
}

fn path() -> std::path::PathBuf {
    crate::config::arkha_file("folders.conf")
}

fn with_store<R>(f: impl FnOnce(&glib::KeyFile) -> R) -> R {
    STORE.with(|s| {
        let mut s = s.borrow_mut();
        let kf = s.get_or_insert_with(|| crate::util::load_settings(&path()).unwrap_or_default());
        f(kf)
    })
}

fn save(kf: &glib::KeyFile) {
    crate::util::save_settings(kf, &path());
}

pub fn get(uri: &str) -> FolderView {
    with_store(|kf| {
        let view = kf.string(uri, "view").ok().map(|v| v.to_string());
        let sort = kf
            .string(uri, "sort_by")
            .ok()
            .map(|by| (by.to_string(), kf.boolean(uri, "sort_desc").unwrap_or(false)));
        let info_panel = kf.boolean(uri, "info_panel").ok();
        FolderView { view, sort, info_panel }
    })
}

/// Removes `uri`'s settings that are the defaults, and the group if none
/// are left; whether anything went.
fn drop_defaults(kf: &glib::KeyFile, uri: &str) -> bool {
    let (view, by, desc) = crate::config::with(|s| (s.default_view.clone(), s.sort_by.clone(), s.sort_desc));
    let mut changed = false;
    if kf.string(uri, "view").is_ok_and(|v| v == view) {
        changed |= kf.remove_key(uri, "view").is_ok();
    }
    let sort_by = kf.string(uri, "sort_by").ok();
    if sort_by.is_none_or(|b| b == by) && kf.boolean(uri, "sort_desc").unwrap_or(false) == desc {
        for k in ["sort_by", "sort_desc"] {
            changed |= kf.remove_key(uri, k).is_ok();
        }
    }
    if kf.boolean(uri, "info_panel").is_ok_and(|on| !on) {
        changed |= kf.remove_key(uri, "info_panel").is_ok();
    }
    if kf.has_group(uri) && kf.keys(uri).is_ok_and(|k| k.is_empty()) {
        changed |= kf.remove_group(uri).is_ok();
    }
    changed
}

/// Sets `uri`'s settings with `f`, keeping only what isn't a default.
fn set(uri: &str, f: impl FnOnce(&glib::KeyFile)) {
    with_store(|kf| {
        f(kf);
        drop_defaults(kf, uri);
        save(kf);
    })
}

pub fn set_view(uri: &str, view: &str) {
    set(uri, |kf| kf.set_string(uri, "view", view));
}

pub fn set_sort(uri: &str, by: &str, desc: bool) {
    set(uri, |kf| {
        kf.set_string(uri, "sort_by", by);
        kf.set_boolean(uri, "sort_desc", desc);
    });
}

pub fn set_info_panel(uri: &str, on: bool) {
    set(uri, |kf| kf.set_boolean(uri, "info_panel", on));
}

/// Forgets `uri`'s remembered view, sorting and info panel.
pub fn forget(uri: &str) {
    with_store(|kf| {
        if kf.remove_group(uri).is_ok() {
            save(kf);
        }
    })
}

/// Remembered settings follow their folders, as moved, renamed or
/// deleted in Arkha: (URI before, URI now, None if deleted). Moved, a
/// folder's settings and those of the folders in it go along; deleted,
/// they go (kept by its URI, a renamed folder lost them, and a deleted
/// one's stayed in the file for good).
pub fn follow(went: &[(String, Option<String>)]) {
    with_store(|kf| {
        let mut changed = false;
        for g in kf.groups().iter().map(|g| g.to_string()) {
            let hit = went.iter().find_map(|(old, new)| {
                let rest = if g == *old { Some(String::new()) } else { g.strip_prefix(&format!("{old}/")).map(|r| format!("/{r}")) };
                rest.map(|r| new.as_ref().map(|n| format!("{n}{r}")))
            });
            let Some(to) = hit else { continue };
            if let Some(to) = to {
                for k in kf.keys(&g).map(|k| k.iter().map(|k| k.to_string()).collect::<Vec<_>>()).unwrap_or_default() {
                    if let Ok(v) = kf.value(&g, &k) {
                        kf.set_value(&to, &k, &v);
                    }
                }
            }
            changed |= kf.remove_group(&g).is_ok();
        }
        if changed {
            save(kf);
        }
    })
}

/// Number of folders with remembered settings.
pub fn count() -> usize {
    with_store(|kf| kf.groups().len())
}

pub fn forget_all() {
    with_store(|kf| {
        for g in kf.groups().iter() {
            let _ = kf.remove_group(g);
        }
        save(kf);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_what_differs_from_the_defaults() {
        let (view, by, desc) = crate::config::with(|s| (s.default_view.clone(), s.sort_by.clone(), s.sort_desc));
        let other_view = if view == "list" { "icon" } else { "list" };
        let other_by = if by == "mtime" { "name" } else { "mtime" };
        let kf = glib::KeyFile::new();
        // All defaults: the folder goes.
        kf.set_string("a", "view", &view);
        kf.set_string("a", "sort_by", &by);
        kf.set_boolean("a", "sort_desc", desc);
        kf.set_boolean("a", "info_panel", false);
        // Its own view only.
        kf.set_string("b", "view", other_view);
        kf.set_boolean("b", "info_panel", false);
        // Its own sorting and info panel.
        kf.set_string("c", "view", &view);
        kf.set_string("c", "sort_by", other_by);
        kf.set_boolean("c", "sort_desc", desc);
        kf.set_boolean("c", "info_panel", true);
        for g in ["a", "b", "c"] {
            drop_defaults(&kf, g);
        }
        let keys = |g: &str| kf.keys(g).map(|k| k.iter().map(|k| k.to_string()).collect::<Vec<_>>()).unwrap_or_default();
        assert!(!kf.has_group("a"));
        assert_eq!(keys("b"), ["view"]);
        assert_eq!(keys("c"), ["sort_by", "sort_desc", "info_panel"]);
        assert!(!drop_defaults(&kf, "b"), "nothing more to drop");
    }

    #[test]
    fn remembered_and_forgotten() {
        let _ = std::fs::remove_file(path());
        let (view, by, desc) = crate::config::with(|s| (s.default_view.clone(), s.sort_by.clone(), s.sort_desc));
        let other_view = if view == "list" { "icon" } else { "list" };
        set_view("file:///a", other_view);
        set_sort("file:///b", &by, !desc);
        set_info_panel("file:///b", true);
        assert_eq!(get("file:///a"), FolderView { view: Some(other_view.into()), ..Default::default() });
        assert_eq!(get("file:///b"), FolderView { view: None, sort: Some((by, !desc)), info_panel: Some(true) });
        assert_eq!(get("file:///elsewhere"), FolderView::default());
        assert_eq!(count(), 2);
        // Back to the defaults: the folder goes, from the file too.
        set_view("file:///a", &view);
        assert_eq!(count(), 1);
        let kf = glib::KeyFile::new();
        kf.load_from_file(path(), glib::KeyFileFlags::NONE).unwrap();
        assert!(!kf.has_group("file:///a") && kf.has_group("file:///b"));
        forget_all();
        assert_eq!(count(), 0);
        // They follow their folders (in this test: it shares the file).
        // (A view that isn't the default, or it wouldn't be kept.)
        let other = |v: &'static str| -> &'static str { if v == view { other_view } else { v } };
        set_view("file:///p/photos", other("list"));
        set_view("file:///p/photos/2026", other("compact"));
        set_view("file:///p/photos2", other("list"));
        set_view("file:///p/old", other("list"));
        follow(&[("file:///p/photos".into(), Some("file:///q/pictures".into())), ("file:///p/old".into(), None)]);
        assert_eq!(get("file:///q/pictures").view.as_deref(), Some(other("list")));
        assert_eq!(get("file:///q/pictures/2026").view.as_deref(), Some(other("compact")));
        assert_eq!(get("file:///p/photos").view, None);
        // Only that folder and those in it: not one that only starts alike.
        assert_eq!(get("file:///p/photos2").view.as_deref(), Some(other("list")));
        assert_eq!(get("file:///p/old").view, None);
        forget_all();
        let _ = std::fs::remove_file(path());
    }
}
