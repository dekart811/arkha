//! Notes the build for the About window (ARKHA_BUILD: "r142, 6a7d287", the
//! number of commits and the commit), and builds the translations: each
//! po/<language>.po into
//! target/<profile>/locale/<language>/LC_MESSAGES/arkha.mo (with gettext's
//! msgfmt), where Arkha finds them when run from the build folder and the
//! PKGBUILD and install.sh take them from, and lists them for Preferences
//! (src/i18n.rs) by the name in their X-Language-Name header. Without
//! msgfmt, Arkha is built in English only. And it makes the manual's HTML
//! pages (see build/manual.rs).

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "build/manual.rs"]
mod manual;

fn main() {
    println!("cargo:rustc-env=ARKHA_BUILD={}", build_id().unwrap_or_default());
    println!("cargo:rerun-if-changed=po");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let profile = profile_dir();
    manual::build(&profile);
    let mut po: Vec<PathBuf> = std::fs::read_dir("po")
        .map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "po")).collect())
        .unwrap_or_default();
    po.sort();
    let mut languages = vec![];
    for p in &po {
        let code = p.file_stem().unwrap().to_string_lossy().to_string();
        let Some(name) = header(p, "X-Language-Name") else {
            println!("cargo:warning={}: no X-Language-Name header, left out", p.display());
            continue;
        };
        let dir = profile.join("locale").join(&code).join("LC_MESSAGES");
        std::fs::create_dir_all(&dir).unwrap();
        let mo = dir.join("arkha.mo");
        match Command::new("msgfmt").arg("--check").arg("-o").arg(&mo).arg(p).status() {
            Ok(s) if s.success() => {
                // The catalog is outside OUT_DIR, where Cargo doesn't keep
                // track of it: built again whenever it's missing (a build
                // folder reused without its locale/ gave a release tarball
                // without translations). It gets its .po file's time, older
                // than this run, or Cargo would run this again every build.
                if let Ok(t) = std::fs::metadata(p).and_then(|m| m.modified()) {
                    let _ = std::fs::File::options().write(true).open(&mo).and_then(|f| f.set_modified(t));
                }
                println!("cargo:rerun-if-changed={}", mo.display());
                languages.push((code, name));
            }
            Ok(_) => panic!("msgfmt found mistakes in {}", p.display()),
            Err(e) => {
                println!("cargo:warning=msgfmt (gettext) not found ({e}): built without translations");
                break;
            }
        }
    }
    let list: String = languages.iter().map(|(c, n)| format!("({c:?}, {n:?}), ")).collect();
    std::fs::write(
        out.join("languages.rs"),
        format!("/// The translations there are: (code, name in its own language).\npub const LANGUAGES: &[(&str, &str)] = &[{list}];\n"),
    )
    .unwrap();
}

/// Where Cargo puts the program: target/<profile>, CARGO_TARGET_DIR's if set
/// (the PKGBUILD sets it). Not worked out from OUT_DIR: since Rust 1.100
/// Cargo keeps a build script's OUT_DIR in a build folder of its own, apart
/// from the program, and the translations went there (the first 1.0.0
/// prebuilt tarball had none).
fn profile_dir() -> PathBuf {
    println!("cargo:rerun-if-env-changed=CARGO_TARGET_DIR");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let target = match std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from) {
        Some(t) if t.is_absolute() => t,
        Some(t) => manifest.join(t),
        None => manifest.join("target"),
    };
    target.join(std::env::var("PROFILE").unwrap())
}

/// The build: from ARKHA_BUILD if a packager sets it, else from git in a
/// checkout (the PKGBUILD builds in one), else from the VERSION file that
/// packaging/remote-build.sh writes into the sources it copies
/// ("1.0.0-dev+r142.g6a7d287"). None for sources with neither.
fn build_id() -> Option<String> {
    println!("cargo:rerun-if-env-changed=ARKHA_BUILD");
    if let Ok(b) = std::env::var("ARKHA_BUILD") {
        return Some(b);
    }
    // Rebuilt after a commit (or a checkout of another one).
    for p in [".git/HEAD", ".git/refs/heads", ".git/packed-refs", "VERSION"] {
        if Path::new(p).exists() {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    let git = |args: &[&str]| {
        let out = Command::new("git").args(args).output().ok().filter(|o| o.status.success())?;
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    if Path::new(".git").exists() {
        if let (Some(n), Some(commit)) = (git(&["rev-list", "--count", "HEAD"]), git(&["rev-parse", "--short", "HEAD"])) {
            return Some(format!("r{n}, {commit}"));
        }
    }
    let version = std::fs::read_to_string("VERSION").ok()?;
    let (count, commit) = version.trim().split_once('+')?.1.split_once(".g")?;
    Some(format!("{count}, {}", commit.trim_end_matches(".dirty")))
}

/// A header field of a .po file (in its first, empty msgid's msgstr).
fn header(po: &Path, field: &str) -> Option<String> {
    let text = std::fs::read_to_string(po).ok()?;
    let prefix = format!("\"{field}: ");
    let line = text.lines().take(40).find(|l| l.starts_with(&prefix))?;
    Some(line[prefix.len()..].trim_end_matches('"').trim_end_matches("\\n").trim().to_string())
}
