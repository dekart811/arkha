//! The manual: docs/manual/<language>/*.md into HTML pages in
//! target/<profile>/manual/<language>/, with docs/manual/style.css and
//! docs/manual/images/ beside them, where Help → Manual opens it when Arkha
//! runs from the build folder and the packages take it from (installed in
//! <prefix>/share/arkha/manual).
//!
//! A language's pages come in the order its index.md links to them (its
//! table of contents, which GitHub shows too). Links between the pages are
//! written as to the .md files, so that they work on GitHub, and point to
//! the .html files here. Headings get GitHub's anchors ("Split view" →
//! #split-view), so "views.md#split-view" works in both. Links to pages or
//! anchors that aren't there, and to anything outside the manual, are
//! reported as warnings. Pictures are the README's screenshots
//! (docs/screenshots, written as "../../screenshots/…", which GitHub shows
//! too) or the manual's own (docs/manual/images, "../images/…"), copied into
//! the manual's images/.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pulldown_cmark::{CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

const SOURCE: &str = "docs/manual";
const SCREENSHOTS: &str = "docs/screenshots";

pub fn build(profile: &Path) {
    println!("cargo:rerun-if-changed={SOURCE}");
    println!("cargo:rerun-if-changed={SCREENSHOTS}");
    let out = profile.join("manual");
    // Made anew: pages taken out of the manual go from it too.
    let _ = std::fs::remove_dir_all(&out);
    let Ok(dirs) = std::fs::read_dir(SOURCE) else {
        println!("cargo:rustc-env=ARKHA_MANUAL_LANGUAGES=");
        return;
    };
    std::fs::create_dir_all(&out).unwrap();
    let mut dirs: Vec<PathBuf> = dirs.flatten().map(|e| e.path()).filter(|p| p.join("index.md").is_file()).collect();
    dirs.sort();
    // The languages it's in, for Help → Manual's online pages (see i18n.rs).
    let languages: Vec<String> = dirs.iter().map(|d| d.file_name().unwrap().to_string_lossy().to_string()).collect();
    println!("cargo:rustc-env=ARKHA_MANUAL_LANGUAGES={}", languages.join(","));
    for dir in dirs {
        language(&dir, &out.join(dir.file_name().unwrap()));
    }
    copy(&Path::new(SOURCE).join("style.css"), &out.join("style.css"));
    // Arkha's icon, at the top of every page.
    copy(Path::new("data/arkha.svg"), &out.join("images/arkha.svg"));
    for dir in [Path::new(SOURCE).join("images"), PathBuf::from(SCREENSHOTS)] {
        if let Ok(images) = std::fs::read_dir(dir) {
            for image in images.flatten() {
                copy(&image.path(), &out.join("images").join(image.file_name()));
            }
        }
    }
}

fn copy(from: &Path, to: &Path) {
    if from.is_file() {
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(from, to).unwrap_or_else(|e| panic!("{}: {e}", from.display()));
    }
}

struct Page {
    /// "views" for views.md.
    name: String,
    text: String,
    /// Its first heading.
    title: String,
    /// Its headings' anchors.
    anchors: Vec<String>,
}

/// One language's pages, from `dir` (docs/manual/en) into `out`.
fn language(dir: &Path, out: &Path) {
    let lang = dir.file_name().unwrap().to_string_lossy().to_string();
    let read = |name: &str| {
        let path = dir.join(format!("{name}.md"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let (title, anchors) = headings(&text);
        if title.is_empty() {
            println!("cargo:warning={}: no heading for its title", path.display());
        }
        Page { name: name.to_string(), text, title, anchors }
    };
    let index = read("index");
    let mut names = vec!["index".to_string()];
    for dest in links(&index.text) {
        if let Some(name) = page_name(&dest) {
            if !names.contains(&name) && dir.join(format!("{name}.md")).is_file() {
                names.push(name);
            }
        }
    }
    let mut pages = vec![index];
    pages.extend(names[1..].iter().map(|n| read(n)));
    // Pages the contents leave out: still made, after the others.
    let mut rest: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".md").map(str::to_string))
        .filter(|n| !names.contains(n))
        .collect();
    rest.sort();
    for name in rest {
        println!("cargo:warning={}/{name}.md: not in the contents (index.md)", dir.display());
        pages.push(read(&name));
    }

    let anchors: HashMap<&str, &[String]> = pages.iter().map(|p| (p.name.as_str(), p.anchors.as_slice())).collect();
    std::fs::create_dir_all(out).unwrap();
    for (i, page) in pages.iter().enumerate() {
        let body = render(&page.text, |dest| {
            let fixed = link(dest, &page.name, &anchors);
            if let Err(why) = &fixed {
                println!("cargo:warning={}/{}.md: link to {dest}: {why}", dir.display(), page.name);
            }
            fixed.unwrap_or_else(|_| dest.to_string())
        })
        .replace("\"../../screenshots/", "\"../images/");
        let prev = i.checked_sub(1).map(|j| &pages[j]);
        let next = pages.get(i + 1);
        let html = template(&lang, &pages, page, prev, next, &body);
        std::fs::write(out.join(format!("{}.html", page.name)), html).unwrap();
    }
}

fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_SMART_PUNCTUATION
}

/// The page's title (its first heading) and its headings' anchors.
fn headings(text: &str) -> (String, Vec<String>) {
    let mut title = String::new();
    let mut anchors = vec![];
    let mut current: Option<String> = None;
    for event in Parser::new_ext(text, options()) {
        match event {
            Event::Start(Tag::Heading { .. }) => current = Some(String::new()),
            Event::Text(t) | Event::Code(t) if current.is_some() => current.as_mut().unwrap().push_str(&t),
            Event::End(TagEnd::Heading(_)) => {
                let text = current.take().unwrap_or_default();
                if title.is_empty() {
                    title = text.clone();
                }
                anchors.push(unique(slug(&text), &anchors));
            }
            _ => {}
        }
    }
    (title, anchors)
}

/// GitHub's anchor for a heading: lower case, without punctuation, spaces
/// as hyphens.
fn slug(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// A second "Examples" heading is #examples-1, as on GitHub.
fn unique(slug: String, taken: &[String]) -> String {
    if !taken.contains(&slug) {
        return slug;
    }
    (1..).map(|n| format!("{slug}-{n}")).find(|s| !taken.contains(s)).unwrap()
}

/// Where the links in `text` go, in order.
fn links(text: &str) -> Vec<String> {
    Parser::new_ext(text, options())
        .filter_map(|e| match e {
            Event::Start(Tag::Link { dest_url, .. }) => Some(dest_url.to_string()),
            _ => None,
        })
        .collect()
}

/// "views" for a link to "views.md" or "views.md#split-view"; None for
/// other links.
fn page_name(dest: &str) -> Option<String> {
    let path = dest.split('#').next()?;
    let name = path.strip_suffix(".md")?;
    (!name.contains('/') && !name.contains(':')).then(|| name.to_string())
}

/// A link as it goes in the HTML: to the .html page for a .md one, checked
/// against the pages and their anchors.
fn link(dest: &str, from: &str, anchors: &HashMap<&str, &[String]>) -> Result<String, &'static str> {
    if dest.contains("://") || dest.starts_with("mailto:") {
        return Ok(dest.to_string());
    }
    if dest.starts_with("../images/") {
        return Ok(dest.to_string());
    }
    let (path, anchor) = match dest.split_once('#') {
        Some((p, a)) => (p, Some(a)),
        None => (dest, None),
    };
    let target = if path.is_empty() {
        from.to_string()
    } else {
        page_name(dest).ok_or("outside the manual (link to the web address instead)")?
    };
    let Some(theirs) = anchors.get(target.as_str()) else { return Err("no such page") };
    if let Some(a) = anchor {
        if !theirs.iter().any(|t| t == a) {
            return Err("no such heading on that page");
        }
    }
    Ok(match (path.is_empty(), anchor) {
        (true, Some(a)) => format!("#{a}"),
        (_, Some(a)) => format!("{target}.html#{a}"),
        (_, None) => format!("{target}.html"),
    })
}

/// The page's Markdown as HTML, its links through `fix` and its headings
/// with their anchors.
fn render(text: &str, fix: impl Fn(&str) -> String) -> String {
    let mut events: Vec<Event> = vec![];
    let mut heading: Option<(HeadingLevel, Vec<Event>, String)> = None;
    let mut taken: Vec<String> = vec![];
    for event in Parser::new_ext(text, options()) {
        let event = match event {
            Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
                Event::Start(Tag::Link { link_type, dest_url: CowStr::from(fix(&dest_url)), title, id })
            }
            e => e,
        };
        match event {
            Event::Start(Tag::Heading { level, .. }) => heading = Some((level, vec![], String::new())),
            Event::End(TagEnd::Heading(_)) => {
                let (level, inner, plain) = heading.take().unwrap();
                let anchor = unique(slug(&plain), &taken);
                let mut html = String::new();
                pulldown_cmark::html::push_html(&mut html, inner.into_iter());
                events.push(Event::Html(CowStr::from(format!("<{level} id=\"{anchor}\">{html}</{level}>\n"))));
                taken.push(anchor);
            }
            e => match &mut heading {
                Some((_, inner, plain)) => {
                    if let Event::Text(t) | Event::Code(t) = &e {
                        plain.push_str(t);
                    }
                    inner.push(e);
                }
                None => events.push(e),
            },
        }
    }
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events.into_iter());
    html
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn template(lang: &str, pages: &[Page], page: &Page, prev: Option<&Page>, next: Option<&Page>, body: &str) -> String {
    let manual = escape(&pages[0].title);
    let title = if page.name == "index" { manual.clone() } else { format!("{} – {manual}", escape(&page.title)) };
    let contents: String = pages[1..]
        .iter()
        .map(|p| {
            let current = if p.name == page.name { " aria-current=\"page\"" } else { "" };
            format!("<li><a href=\"{}.html\"{current}>{}</a></li>\n", p.name, escape(&p.title))
        })
        .collect();
    let step = |p: Option<&Page>, rel: &str, arrow_before: bool| match p {
        Some(p) if arrow_before => format!("<a rel=\"{rel}\" href=\"{}.html\">← {}</a>", p.name, escape(&p.title)),
        Some(p) => format!("<a rel=\"{rel}\" href=\"{}.html\">{} →</a>", p.name, escape(&p.title)),
        None => "<span></span>".to_string(),
    };
    let version = env!("CARGO_PKG_VERSION");
    format!(
        "<!DOCTYPE html>
<html lang=\"{lang}\">
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>{title}</title>
<link rel=\"stylesheet\" href=\"../style.css\">
</head>
<body>
<nav class=\"contents\">
<a class=\"manual\" href=\"index.html\"><img src=\"../images/arkha.svg\" alt=\"\"> {manual}</a>
<ol>
{contents}</ol>
<p class=\"version\">Arkha {version}</p>
</nav>
<main>
{body}<nav class=\"steps\">{}{}</nav>
</main>
</body>
</html>
",
        step(prev, "prev", true),
        step(next, "next", false),
    )
}
