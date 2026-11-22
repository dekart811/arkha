//! Thumbnails that go with their files: moved along when Arkha moves or
//! renames a file, removed when it deletes one (also by emptying the
//! trash). The freedesktop.org thumbnail spec names a thumbnail after its
//! file's URI and keeps that URI in it (Thumb::URI), so a moved file's
//! thumbnail would otherwise be made again under the new name, and the old
//! one stay behind for good. Files trashed keep theirs (restored, they're
//! back at their old URI); files changed by other programs aren't known.

use gio::prelude::*;
use std::path::{Path, PathBuf};

/// The sizes the spec keeps thumbnails in.
const SIZES: [&str; 4] = ["normal", "large", "x-large", "xx-large"];

/// Where thumbnails are kept.
pub fn root() -> PathBuf {
    glib::user_cache_dir().join("thumbnails")
}

/// A thumbnail's file name: the MD5 of the file's URI.
pub(crate) fn name(uri: &str) -> String {
    let md5 = glib::compute_checksum_for_data(glib::ChecksumType::Md5, uri.as_bytes()).unwrap_or_default();
    format!("{md5}.png")
}

/// Arkha's note that a file's thumbnail couldn't be made.
fn fail_note(root: &Path, uri: &str) -> PathBuf {
    root.join("fail").join("arkha").join(name(uri))
}

/// Tells Arkha's views which thumbnail files changed: `written` (the URI
/// of the file they're for, their path), `removed` (paths). They keep
/// loaded thumbnails in memory by path, and a file's thumbnail has the
/// same path as the next file's of that name: a folder.jpg deleted and
/// another moved in under that name showed the old picture. On the main
/// thread (these run on others).
fn tell(written: Vec<(String, PathBuf)>, removed: Vec<PathBuf>) {
    if written.is_empty() && removed.is_empty() {
        return;
    }
    let s = |p: &Path| p.to_string_lossy().to_string();
    let written: Vec<(String, String)> = written.iter().map(|(u, p)| (u.clone(), s(p))).collect();
    let removed: Vec<String> = removed.iter().map(|p| s(p)).collect();
    glib::MainContext::default().invoke(move || crate::icons::thumbnails_changed(&written, &removed));
}

/// The thumbnails (each size) and failure note of `uri` removed: the file
/// was deleted.
pub fn forget(root: &Path, uri: &str) {
    tell(vec![], remove(root, uri));
}

/// Whether a move or copy from `old` to `new` takes thumbnails along. Not
/// out of the trash or into it: a trashed file keeps its thumbnail under
/// its URI before (restored, it's back there; a restore was taking it
/// away, and it was made again).
fn follows(old: &str, new: &str) -> bool {
    let trash = |u: &str| u.starts_with("trash:");
    old != new && !trash(old) && !trash(new)
}

/// `forget` without telling the views: the paths removed.
fn remove(root: &Path, uri: &str) -> Vec<PathBuf> {
    let n = name(uri);
    let mut gone: Vec<PathBuf> = SIZES.iter().map(|s| root.join(s).join(&n)).filter(|p| std::fs::remove_file(p).is_ok()).collect();
    if std::fs::remove_file(fail_note(root, uri)).is_ok() {
        gone.push(fail_note(root, uri));
    }
    gone
}

/// The thumbnails of copied items (URI of the original, URI of the copy)
/// given to the copies, the originals keeping theirs: in a move between
/// drives, which copies file by file and deletes the originals at the end
/// (cancelled, or a copy found spoilt, an original stays), the copies get
/// them as they arrive; the originals' go once they're deleted (forget).
pub fn copied(root: &Path, items: &[(String, String)]) {
    let mut written = vec![];
    let mut removed = vec![];
    for (old, new) in items.iter().filter(|(o, n)| follows(o, n)) {
        removed.extend(remove(root, new));
        for s in SIZES {
            if let Some(png) = std::fs::read(root.join(s).join(name(old))).ok().and_then(|b| with_uri(&b, old, new)) {
                let path = root.join(s).join(name(new));
                write(&path, &png);
                written.push((new.clone(), path));
            }
        }
    }
    tell(written, removed);
}

/// The thumbnails of moved or renamed items (URI before, URI now) moved
/// along, also those of everything in a moved folder (found by going
/// through it at its new place). All are read before any is written, so
/// renames like a → b, b → a keep each its own. A thumbnail whose
/// Thumb::URI isn't the item's is left; failure notes go (the file is
/// tried again at its new place), as do the new names' old thumbnails
/// (of items the move replaced). Files on network places have thumbnails
/// too, when Preferences → Preview says so (also those of a phone, when
/// GVfs gives them a path).
pub fn moved(root: &Path, items: &[(String, String)]) {
    let mut all = items.to_vec();
    for (old, new) in items.iter().filter(|(o, n)| follows(o, n)) {
        below(old, new, &mut all);
    }
    move_all(root, all);
}

/// `moved` for the items themselves, not what's in a folder among them: a
/// rename can't wait for going through it (moved_inside does), or for
/// asking a network place what each is.
pub fn moved_items(root: &Path, items: &[(String, String)]) {
    move_all(root, items.to_vec());
}

/// `moved` for what's in the folders among `items` (the others are left).
pub fn moved_inside(root: &Path, items: &[(String, String)]) {
    let mut all = vec![];
    for (old, new) in items.iter().filter(|(o, n)| follows(o, n)) {
        below(old, new, &mut all);
    }
    move_all(root, all);
}

fn move_all(root: &Path, mut all: Vec<(String, String)>) {
    all.retain(|(old, new)| follows(old, new));
    if all.is_empty() {
        return;
    }
    let mut removed = vec![];
    let mut keep = vec![];
    for (old, new) in &all {
        for s in SIZES {
            if let Some(png) = std::fs::read(root.join(s).join(name(old))).ok().and_then(|b| with_uri(&b, old, new)) {
                keep.push((root.join(s).join(name(new)), png, new.clone()));
            }
        }
    }
    for (old, new) in &all {
        removed.extend(remove(root, old));
        removed.extend(remove(root, new));
    }
    let mut written = vec![];
    for (path, png, new) in keep {
        write(&path, &png);
        written.push((new, path));
    }
    tell(written, removed);
}

/// (URI before, URI now) of everything in the folder `new` (moved there
/// from `old`), added to `out`. Links aren't followed. On a network place
/// it goes through GVfs's path for it, but names the items by the place's
/// own URIs (sftp://…), as their thumbnails are.
fn below(old: &str, new: &str, out: &mut Vec<(String, String)>) {
    let new = gio::File::for_uri(new);
    let Some(dir) = new.path() else { return };
    if !std::fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir()) {
        return;
    }
    let old = gio::File::for_uri(old);
    let mut todo = vec![dir.clone()];
    while let Some(d) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let Ok(rel) = p.strip_prefix(&dir) else { continue };
            out.push((old.resolve_relative_path(rel).uri().to_string(), new.resolve_relative_path(rel).uri().to_string()));
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                todo.push(p);
            }
        }
    }
}

/// Writes a thumbnail as the spec asks: readable only by its owner, and
/// in one piece (written beside it, then renamed).
fn write(path: &Path, png: &[u8]) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    // Its own temporary name: moves and renames run on threads of their own,
    // and with one name for all, two at once wrote into each other's file.
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if path.parent().is_none() {
        return;
    }
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("arkha-{}-{n}.tmp", std::process::id()));
    let written = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp).and_then(|mut f| f.write_all(png));
    if written.and_then(|_| std::fs::rename(&tmp, path)).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// The PNG `png` with its Thumb::URI `new` instead of `old`; None if it
/// isn't a whole PNG or its Thumb::URI isn't `old`. Only that text chunk is
/// rewritten: the picture isn't decoded.
fn with_uri(png: &[u8], old: &str, new: &str) -> Option<Vec<u8>> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    let mut rest = png.strip_prefix(SIG)?;
    let mut out = Vec::with_capacity(png.len() + new.len());
    out.extend_from_slice(SIG);
    let (mut found, mut ended) = (false, false);
    while rest.len() >= 12 {
        let len = u32::from_be_bytes(rest[..4].try_into().ok()?) as usize;
        let chunk = rest.get(..12 + len)?;
        let (kind, data) = (&chunk[4..8], &chunk[8..8 + len]);
        if kind == b"tEXt" && data.strip_prefix(b"Thumb::URI\0") == Some(old.as_bytes()) {
            let mut text = b"Thumb::URI\0".to_vec();
            text.extend_from_slice(new.as_bytes());
            out.extend_from_slice(&(text.len() as u32).to_be_bytes());
            let mut crc = crc32fast::Hasher::new();
            crc.update(b"tEXt");
            crc.update(&text);
            out.extend_from_slice(b"tEXt");
            out.extend_from_slice(&text);
            out.extend_from_slice(&crc.finalize().to_be_bytes());
            found = true;
        } else {
            out.extend_from_slice(chunk);
        }
        rest = &rest[12 + len..];
        if kind == b"IEND" {
            ended = true;
            break;
        }
    }
    (found && ended).then_some(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn chunk(kind: &[u8], data: &[u8]) -> Vec<u8> {
        let mut c = (data.len() as u32).to_be_bytes().to_vec();
        c.extend_from_slice(kind);
        c.extend_from_slice(data);
        let mut crc = crc32fast::Hasher::new();
        crc.update(kind);
        crc.update(data);
        c.extend_from_slice(&crc.finalize().to_be_bytes());
        c
    }

    /// A thumbnail PNG (as far as the chunks go) for `uri`.
    pub fn thumb(uri: &str) -> Vec<u8> {
        let mut p = b"\x89PNG\r\n\x1a\n".to_vec();
        p.extend(chunk(b"IHDR", &[0; 13]));
        p.extend(chunk(b"tEXt", format!("Thumb::URI\0{uri}").as_bytes()));
        p.extend(chunk(b"tEXt", b"Thumb::MTime\x001700000000"));
        p.extend(chunk(b"IDAT", b"picture"));
        p.extend(chunk(b"IEND", b""));
        p
    }

    fn put(root: &Path, size: &str, uri: &str) {
        std::fs::create_dir_all(root.join(size)).unwrap();
        std::fs::write(root.join(size).join(name(uri)), thumb(uri)).unwrap();
    }

    /// Whether the thumbnail kept for `uri` in `size` is one for it.
    fn has(root: &Path, size: &str, uri: &str) -> bool {
        std::fs::read(root.join(size).join(name(uri))).is_ok_and(|b| b == thumb(uri))
    }

    #[test]
    fn copies_get_their_own() {
        let t = TestDir::new("thumbcopy");
        let root = t.0.join("cache");
        let (a, b) = ("file:///d/a.mp4".to_string(), "file:///e/a.mp4".to_string());
        put(&root, "large", &a);
        copied(&root, &[(a.clone(), b.clone())]);
        assert!(has(&root, "large", &a) && has(&root, "large", &b));
    }

    #[test]
    fn names_as_the_spec_has_them() {
        // The spec's example.
        assert_eq!(name("file:///home/jens/photos/me.png"), "c6ee772d9e49320e97ec29a7eb5b1697.png");
    }

    #[test]
    fn uri_rewritten_in_place() {
        let old = thumb("file:///a/x.mp4");
        let new = with_uri(&old, "file:///a/x.mp4", "file:///b/longer name.mp4").unwrap();
        assert_eq!(new, thumb("file:///b/longer name.mp4"));
        // Not this file's thumbnail, or no PNG: left alone.
        assert!(with_uri(&old, "file:///a/y.mp4", "file:///b/y.mp4").is_none());
        assert!(with_uri(b"GIF89a", "file:///a/x.mp4", "file:///b/x.mp4").is_none());
        assert!(with_uri(&old[..old.len() - 20], "file:///a/x.mp4", "file:///b/x.mp4").is_none());
    }

    #[test]
    fn thumbnails_move_with_files_and_folders() {
        let t = TestDir::new("thumbs");
        let root = t.0.join("cache");
        let files = t.0.join("files");
        std::fs::create_dir_all(files.join("new/sub")).unwrap();
        std::fs::write(files.join("new/sub/clip.mp4"), b"").unwrap();
        std::fs::write(files.join("b.mp4"), b"").unwrap();
        let u = |p: &str| gio::File::for_path(files.join(p)).uri().to_string();
        put(&root, "normal", &u("a.mp4"));
        put(&root, "large", &u("a.mp4"));
        put(&root, "large", &u("old/sub/clip.mp4"));
        std::fs::create_dir_all(root.join("fail/arkha")).unwrap();
        std::fs::write(fail_note(&root, &u("a.mp4")), b"note").unwrap();
        moved(&root, &[(u("a.mp4"), u("b.mp4")), (u("old"), u("new"))]);
        assert!(has(&root, "normal", &u("b.mp4")) && has(&root, "large", &u("b.mp4")));
        assert!(!root.join("normal").join(name(&u("a.mp4"))).exists());
        assert!(!fail_note(&root, &u("a.mp4")).exists());
        // In the moved folder.
        assert!(has(&root, "large", &u("new/sub/clip.mp4")));
        assert!(!root.join("large").join(name(&u("old/sub/clip.mp4"))).exists());
        // Swapped names keep each its own: b's picture (the normal size
        // only it has) goes to the clip's name, the clip's (large) to b's.
        std::fs::remove_file(root.join("large").join(name(&u("b.mp4")))).unwrap();
        moved(&root, &[(u("b.mp4"), u("new/sub/clip.mp4")), (u("new/sub/clip.mp4"), u("b.mp4"))]);
        assert!(has(&root, "normal", &u("new/sub/clip.mp4")) && !root.join("large").join(name(&u("new/sub/clip.mp4"))).exists());
        assert!(has(&root, "large", &u("b.mp4")) && !root.join("normal").join(name(&u("b.mp4"))).exists());
        // Deleted: gone, every size and the note.
        std::fs::write(fail_note(&root, &u("b.mp4")), b"note").unwrap();
        forget(&root, &u("b.mp4"));
        assert!(SIZES.iter().all(|s| !root.join(s).join(name(&u("b.mp4"))).exists()));
        assert!(!fail_note(&root, &u("b.mp4")).exists());
    }

    #[test]
    fn network_places_have_thumbnails_too() {
        // Preferences → Preview → Show thumbnails: Always.
        let t = TestDir::new("thumbsnet");
        let root = t.0.join("cache");
        let (a, b, c) = ("sftp://nas/photos/a%20b.jpg", "sftp://nas/photos/c.jpg", "smb://nas/share/c.jpg");
        put(&root, "large", a);
        moved(&root, &[(a.into(), b.into())]);
        assert!(has(&root, "large", b) && !root.join("large").join(name(a)).exists());
        // Between a network place and a local folder, both ways.
        let local = gio::File::for_path(t.0.join("c.jpg")).uri().to_string();
        moved(&root, &[(b.into(), local.clone())]);
        assert!(has(&root, "large", &local) && !root.join("large").join(name(b)).exists());
        copied(&root, &[(local.clone(), c.into())]);
        assert!(has(&root, "large", &local) && has(&root, "large", c));
        forget(&root, c);
        assert!(!root.join("large").join(name(c)).exists());
        // Its own URI, nothing done.
        moved(&root, &[(local.clone(), local.clone())]);
        assert!(has(&root, "large", &local));
    }

    #[test]
    fn the_trash_leaves_thumbnails_alone() {
        let t = TestDir::new("thumbstrash");
        let root = t.0.join("cache");
        std::fs::create_dir_all(t.0.join("photos/sub")).unwrap();
        let u = |p: &str| gio::File::for_path(t.0.join(p)).uri().to_string();
        // Kept under the URI before for a restore, which puts the file
        // (or folder) back there: not taken away by it.
        put(&root, "large", &u("photos/a.jpg"));
        put(&root, "large", &u("photos/sub/b.jpg"));
        moved(&root, &[("trash:///a.jpg".into(), u("photos/a.jpg")), ("trash:///sub".into(), u("photos/sub"))]);
        moved_items(&root, &[("trash:///a.jpg".into(), u("photos/a.jpg"))]);
        moved_inside(&root, &[("trash:///sub".into(), u("photos/sub"))]);
        copied(&root, &[("trash:///a.jpg".into(), u("photos/a.jpg"))]);
        assert!(has(&root, "large", &u("photos/a.jpg")) && has(&root, "large", &u("photos/sub/b.jpg")));
        // Nor moved to a trash URI.
        moved(&root, &[(u("photos/a.jpg"), "trash:///a.jpg".into())]);
        assert!(has(&root, "large", &u("photos/a.jpg")) && !root.join("large").join(name("trash:///a.jpg")).exists());
    }

    #[test]
    fn renamed_items_now_their_contents_later() {
        let t = TestDir::new("thumbsplit");
        let root = t.0.join("cache");
        let files = t.0.join("files");
        std::fs::create_dir_all(files.join("new")).unwrap();
        std::fs::write(files.join("new/clip.mp4"), b"").unwrap();
        let u = |p: &str| gio::File::for_path(files.join(p)).uri().to_string();
        put(&root, "large", &u("a.mp4"));
        put(&root, "large", &u("old/clip.mp4"));
        let items = [(u("a.mp4"), u("b.mp4")), (u("old"), u("new"))];
        moved_items(&root, &items);
        assert!(has(&root, "large", &u("b.mp4")));
        assert!(has(&root, "large", &u("old/clip.mp4")) && !root.join("large").join(name(&u("new/clip.mp4"))).exists());
        moved_inside(&root, &items);
        assert!(has(&root, "large", &u("new/clip.mp4")) && !root.join("large").join(name(&u("old/clip.mp4"))).exists());
        // A file among them isn't gone through (b.mp4 isn't a folder), and
        // a file name with what URIs escape keeps its escapes.
        assert!(has(&root, "large", &u("b.mp4")));
        std::fs::write(files.join("new/x y#.mp4"), b"").unwrap();
        put(&root, "large", &u("old/x y#.mp4"));
        moved_inside(&root, &[(u("old"), u("new"))]);
        assert!(has(&root, "large", &u("new/x y#.mp4")));
    }
}
