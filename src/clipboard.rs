//! Copy/cut/paste of files, interoperable with Caja, Nautilus and friends.

use gtk::prelude::*;
use std::cell::RefCell;

const MATE: &str = "x-special/mate-copied-files";
const GNOME: &str = "x-special/gnome-copied-files";
const URIS: &str = "text/uri-list";

thread_local! {
    /// URIs we last cut, so views can dim them and we can clear after paste.
    static CUT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn clipboard() -> gtk::Clipboard {
    gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD)
}

pub fn set(uris: Vec<String>, cut: bool) {
    let targets = [
        gtk::TargetEntry::new(MATE, gtk::TargetFlags::empty(), 0),
        gtk::TargetEntry::new(GNOME, gtk::TargetFlags::empty(), 0),
        gtk::TargetEntry::new(URIS, gtk::TargetFlags::empty(), 1),
        gtk::TargetEntry::new("UTF8_STRING", gtk::TargetFlags::empty(), 2),
        gtk::TargetEntry::new("text/plain;charset=utf-8", gtk::TargetFlags::empty(), 2),
        gtk::TargetEntry::new("text/plain", gtk::TargetFlags::empty(), 2),
    ];
    let data = uris.clone();
    clipboard().set_with_data(&targets, move |_, sel, info| match info {
        0 => {
            let s = format!("{}\n{}", if cut { "cut" } else { "copy" }, data.join("\n"));
            sel.set(&sel.target(), 8, s.as_bytes());
        }
        1 => {
            let refs: Vec<&str> = data.iter().map(|s| s.as_str()).collect();
            sel.set_uris(&refs);
        }
        _ => {
            let text: Vec<String> = data
                .iter()
                .map(|u| {
                    gio::File::for_uri(u)
                        .path()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| u.clone())
                })
                .collect();
            sel.set_text(&text.join("\n"));
        }
    });
    CUT.with(|c| *c.borrow_mut() = if cut { uris } else { vec![] });
}

/// Whether any items are cut (see is_cut).
pub fn any_cut() -> bool {
    CUT.with(|c| !c.borrow().is_empty())
}

pub fn is_cut(uri: &str) -> bool {
    CUT.with(|c| c.borrow().iter().any(|u| u == uri))
}

pub fn clear_after_cut() {
    CUT.with(|c| c.borrow_mut().clear());
    clipboard().clear();
}

fn parse_special(data: &[u8]) -> Option<(bool, Vec<String>)> {
    let s = String::from_utf8_lossy(data);
    let mut lines = s.lines();
    let first = lines.next()?.trim();
    let cut = match first {
        "cut" => true,
        "copy" => false,
        _ => return None,
    };
    let uris: Vec<String> = lines
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if uris.is_empty() {
        None
    } else {
        Some((cut, uris))
    }
}

/// Read file URIs from the clipboard. Callback gets (is_cut, uris).
pub fn get(cb: impl FnOnce(bool, Vec<String>) + 'static) {
    clipboard().request_contents(&gdk::Atom::intern(MATE), move |c, sel| {
        if let Some((cut, uris)) = parse_special(&sel.data()) {
            cb(cut, uris);
            return;
        }
        c.request_contents(&gdk::Atom::intern(GNOME), move |c, sel| {
            if let Some((cut, uris)) = parse_special(&sel.data()) {
                cb(cut, uris);
                return;
            }
            c.request_contents(&gdk::Atom::intern(URIS), move |c, sel| {
                let uris: Vec<String> = sel.uris().iter().map(|s| s.to_string()).collect();
                if !uris.is_empty() {
                    cb(false, uris);
                    return;
                }
                // Plain text with absolute paths, one per line.
                c.request_text(move |_, text| {
                    let uris: Vec<String> = text
                        .unwrap_or_default()
                        .lines()
                        .map(|l| l.trim())
                        .filter(|l| l.starts_with('/') && std::path::Path::new(l).exists())
                        .map(|l| gio::File::for_path(l).uri().to_string())
                        .collect();
                    cb(false, uris);
                });
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uris(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn special_formats() {
        assert_eq!(parse_special(b"copy\nfile:///a\nfile:///b%20c\n"), Some((false, uris(&["file:///a", "file:///b%20c"]))));
        assert_eq!(parse_special(b"cut\r\nfile:///a\r\n"), Some((true, uris(&["file:///a"]))));
        assert_eq!(parse_special(b"copy\n\n  file:///a  \n"), Some((false, uris(&["file:///a"]))));
        assert_eq!(parse_special(b"copy\n"), None);
        assert_eq!(parse_special(b""), None);
        assert_eq!(parse_special(b"move\nfile:///a"), None);
        // Nautilus' plain-text variant has a header line first.
        assert_eq!(parse_special(b"x-special/nautilus-clipboard\ncopy\nfile:///a"), None);
    }

    #[test]
    fn random_clipboard_data() {
        crate::testutil::cases(3000, |r| {
            let mut data = r.text(40).into_bytes();
            if r.chance(3) {
                data.splice(0..0, if r.chance(2) { b"copy\n".to_vec() } else { b"cut\n".to_vec() });
            }
            if let Some((_, uris)) = parse_special(&data) {
                assert!(!uris.is_empty() && uris.iter().all(|u| !u.is_empty() && u.trim() == u), "{uris:?}");
            }
        });
    }
}
