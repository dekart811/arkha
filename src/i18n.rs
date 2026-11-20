//! Translations: Arkha's texts in the user's language, from gettext
//! catalogs (po/<language>.po, built into arkha.mo files by build.rs). The
//! language is the system's unless one is chosen in Preferences, which then
//! applies to GTK's own texts (dialogs, standard buttons) too.
//!
//! In the code a text to translate is written as tr("…"), one with a number
//! in it as trn("… one", "… several", n); either may hold {name} places
//! that fill() puts values in, so that a translation can order the words as
//! its language needs. tools/update-po.sh collects them for translators.

use std::ffi::{c_char, c_ulong, CStr, CString};
use std::path::PathBuf;

extern "C" {
    fn gettext(msgid: *const c_char) -> *mut c_char;
    fn ngettext(msgid: *const c_char, plural: *const c_char, n: c_ulong) -> *mut c_char;
    fn bindtextdomain(domain: *const c_char, dir: *const c_char) -> *mut c_char;
    fn bind_textdomain_codeset(domain: *const c_char, codeset: *const c_char) -> *mut c_char;
    fn textdomain(domain: *const c_char) -> *mut c_char;
}

const DOMAIN: &CStr = c"arkha";

// LANGUAGES: the translations there are, (code, name in its own language),
// from build.rs.
include!(concat!(env!("OUT_DIR"), "/languages.rs"));

/// Sets up the translations; `language` is the one chosen in Preferences,
/// or "" for the system's. Called first thing, before GTK starts and before
/// any thread: it may set LANGUAGE in the environment.
pub fn init(language: &str) {
    if !language.is_empty() {
        // LANGUAGE picks the messages' language over LANG and LC_MESSAGES,
        // for GTK's catalogs as well as Arkha's.
        std::env::set_var("LANGUAGE", language);
    }
    // SAFETY: plain C calls with valid NUL-terminated strings; nothing else
    // runs yet.
    unsafe {
        libc::setlocale(libc::LC_ALL, c"".as_ptr());
        if let Some(dir) = locale_dir().and_then(|d| CString::new(d.into_os_string().into_encoded_bytes()).ok()) {
            bindtextdomain(DOMAIN.as_ptr(), dir.as_ptr());
        }
        bind_textdomain_codeset(DOMAIN.as_ptr(), c"UTF-8".as_ptr());
        textdomain(DOMAIN.as_ptr());
    }
}

/// Where the catalogs are: next to the program when it's run from the build
/// folder (target/release/locale), else <prefix>/share/locale for one
/// installed in <prefix>/bin (/usr, ~/.local).
fn locale_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bin = exe.parent()?;
    let has_catalogs = |d: &PathBuf| LANGUAGES.iter().any(|(code, _)| d.join(code).join("LC_MESSAGES/arkha.mo").is_file());
    [bin.join("locale"), bin.parent()?.join("share/locale")].into_iter().find(has_catalogs)
}

/// The manual's first page (see build/manual.rs) in the user's language,
/// else in English: next to the program when it's run from the build folder
/// (target/release/manual), else in <prefix>/share/arkha/manual. Err holds
/// the folders looked in, for saying so.
pub fn manual() -> Result<PathBuf, Vec<PathBuf>> {
    let exe = std::env::current_exe().map_err(|_| vec![])?;
    let dirs = manual_dirs(exe.parent().ok_or_else(Vec::new)?);
    // "de_DE.UTF-8", "de_DE", "de", "C": LANGUAGE's first (the language
    // chosen in Preferences is set there), then LC_ALL's, LC_MESSAGES's or
    // LANG's.
    let languages: Vec<String> = glib::language_names().iter().map(|l| l.to_string()).collect();
    manual_in(&dirs, &languages).ok_or(dirs)
}

/// Where the manual may be for a program in `bin`: next to it (the build
/// folder, an unpacked prebuilt tarball), and <prefix>/share/arkha/manual.
/// Not next to it in a bin folder: nothing puts it in /usr/bin, and the
/// message that it isn't installed would name the place.
fn manual_dirs(bin: &std::path::Path) -> Vec<PathBuf> {
    let beside = (bin.file_name() != Some(std::ffi::OsStr::new("bin"))).then(|| bin.join("manual"));
    [beside, bin.parent().map(|p| p.join("share/arkha/manual"))].into_iter().flatten().collect()
}

/// The manual online, for when it isn't installed: its first page on GitHub
/// as it is in this version (a release's tag, a development build's commit),
/// in the user's language if the manual is written in it, else in English.
pub fn online_manual() -> String {
    let languages: Vec<String> = glib::language_names().iter().map(|l| l.to_string()).collect();
    online_manual_for(env!("CARGO_PKG_VERSION"), env!("ARKHA_BUILD"), env!("ARKHA_MANUAL_LANGUAGES"), &languages)
}

/// online_manual for a version ("1.2.0", "1.2.0-dev"), a build ("r223,
/// e150cd3", see build.rs), the manual's languages ("de,en") and the user's.
fn online_manual_for(version: &str, build: &str, written_in: &str, languages: &[String]) -> String {
    let at = if !version.contains('-') {
        format!("v{version}")
    } else {
        build.rsplit_once(", ").map(|(_, commit)| commit.to_string()).unwrap_or_else(|| "master".to_string())
    };
    let written_in: Vec<&str> = written_in.split(',').collect();
    let language = languages.iter().map(String::as_str).find(|l| written_in.contains(l)).unwrap_or("en");
    format!("{}/blob/{at}/docs/manual/{language}/index.md", env!("CARGO_PKG_REPOSITORY"))
}

fn manual_in(dirs: &[PathBuf], languages: &[String]) -> Option<PathBuf> {
    // The first folder with a manual (always in English): the build
    // folder's, not one installed before.
    let dir = dirs.iter().find(|d| d.join("en/index.html").is_file())?;
    let wanted = languages.iter().map(String::as_str).filter(|l| *l != "C" && *l != "POSIX").chain(["en"]);
    wanted.map(|l| dir.join(l).join("index.html")).find(|p| p.is_file())
}

/// `msgid` in the user's language (as it is if there's no translation).
pub fn tr(msgid: &str) -> String {
    // gettext("") is the catalog's header.
    if msgid.is_empty() {
        return String::new();
    }
    let Ok(id) = CString::new(msgid) else { return msgid.to_string() };
    // SAFETY: gettext returns `id` itself or a string in the catalog, which
    // stays loaded; it's copied before `id` is dropped.
    unsafe { CStr::from_ptr(gettext(id.as_ptr())).to_string_lossy().into_owned() }
}

/// The form of a text for the number `n`: `singular` or `plural` in
/// English, as many forms as the language has in a translation.
pub fn trn(singular: &str, plural: &str, n: u64) -> String {
    let (Ok(s), Ok(p)) = (CString::new(singular), CString::new(plural)) else {
        return if n == 1 { singular } else { plural }.to_string();
    };
    // SAFETY: as in tr.
    unsafe { CStr::from_ptr(ngettext(s.as_ptr(), p.as_ptr(), n as c_ulong)).to_string_lossy().into_owned() }
}

/// Marks a text in a constant (a table) for translation: it's translated
/// with tr() where it's shown. As gettext's N_().
pub const fn n_(msgid: &'static str) -> &'static str {
    msgid
}

/// `text` with each {name} replaced by its value.
pub fn fill(text: &str, values: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut out = text.to_string();
    for (name, value) in values {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untranslated_and_filled() {
        assert_eq!(tr("Open"), "Open");
        let empty = "";
        assert_eq!(tr(empty), "", "not the catalog's header");
        assert_eq!(trn("{n} item", "{n} items", 1), "{n} item");
        assert_eq!(trn("{n} item", "{n} items", 5), "{n} items");
        assert_eq!(fill(&trn("{n} item", "{n} items", 5), &[("n", &5)]), "5 items");
        assert_eq!(fill("{a} of {b}, {a}", &[("a", &"one"), ("b", &2)]), "one of 2, one");
    }

    #[test]
    fn online_manual_of_this_version_in_the_language() {
        let langs = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let repo = env!("CARGO_PKG_REPOSITORY");
        let de = langs(&["de_DE.UTF-8", "de_DE", "de", "C"]);
        assert_eq!(online_manual_for("1.2.0", "r230, 1a2b3c4", "de,en", &de), format!("{repo}/blob/v1.2.0/docs/manual/de/index.md"), "a release: its tag");
        assert_eq!(online_manual_for("1.2.0-dev", "r223, e150cd3", "de,en", &de), format!("{repo}/blob/e150cd3/docs/manual/de/index.md"), "a development build: its commit");
        assert_eq!(online_manual_for("1.2.0-dev", "", "de,en", &langs(&["C"])), format!("{repo}/blob/master/docs/manual/en/index.md"), "no build known: master, in English");
        assert_eq!(online_manual_for("1.2.0", "", "de,en", &langs(&["fr_FR", "fr", "C"])), format!("{repo}/blob/v1.2.0/docs/manual/en/index.md"), "not written in French");
    }

    #[test]
    fn manual_beside_the_program_or_in_share() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(manual_dirs(&p("/usr/bin")), [p("/usr/share/arkha/manual")], "installed: not /usr/bin/manual");
        assert_eq!(manual_dirs(&p("/home/u/.local/bin")), [p("/home/u/.local/share/arkha/manual")]);
        assert_eq!(manual_dirs(&p("/src/arkha/target/release")), [p("/src/arkha/target/release/manual"), p("/src/arkha/target/share/arkha/manual")]);
        assert_eq!(manual_dirs(&p("/opt/arkha-1.2.0-x86_64")), [p("/opt/arkha-1.2.0-x86_64/manual"), p("/opt/share/arkha/manual")], "an unpacked tarball");
    }

    #[test]
    fn manual_in_the_language_else_english() {
        let t = crate::testutil::TestDir::new("i18n-manual");
        let (build, installed) = (t.0.join("build"), t.0.join("share"));
        let page = |d: &PathBuf, l: &str| {
            std::fs::create_dir_all(d.join(l)).unwrap();
            std::fs::write(d.join(l).join("index.html"), "").unwrap();
            d.join(l).join("index.html")
        };
        let dirs = [build.clone(), installed.clone()];
        let langs = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(manual_in(&dirs, &langs(&["de_DE.UTF-8", "de_DE", "de", "C"])), None, "none there");
        let en = page(&installed, "en");
        assert_eq!(manual_in(&dirs, &langs(&["de_DE.UTF-8", "de_DE", "de", "C"])), Some(en.clone()), "English for want of German");
        assert_eq!(manual_in(&dirs, &langs(&["C"])), Some(en.clone()));
        let de = page(&installed, "de");
        assert_eq!(manual_in(&dirs, &langs(&["de_DE.UTF-8", "de_DE", "de", "C"])), Some(de), "the language's");
        assert_eq!(manual_in(&dirs, &langs(&["fr_FR", "fr", "C"])), Some(en), "English for want of French");
        let built = page(&build, "en");
        assert_eq!(manual_in(&dirs, &langs(&["de", "C"])), Some(built), "the build folder's, not one installed before");
    }
}
