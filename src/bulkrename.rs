//! "Rename All…": renaming several items at once, the way MATE's caja-rename
//! extension (25.1.1) does it: Case / Insert / Remove / Replace / Enumerate
//! tabs, a live preview, and Apply, which keeps the dialog open. Arkha adds
//! a Regex tab and options to work before/after the file extension.

use crate::i18n::{fill, n_, tr};
use gio::prelude::*;
use gtk::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::{config, icons};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    Title,
    #[default]
    Unchanged,
}

/// The dialog's settings; `Default` is what it starts with (and resets to).
#[derive(Clone, Debug)]
pub struct Options {
    pub case: Case,
    pub insert_text: String,
    pub insert_pos: usize,
    /// Arkha's additions: count Position back from the end, and/or only
    /// within the name before the extension (they combine).
    pub insert_from_end: bool,
    pub insert_before_ext: bool,
    pub remove_from: i32,
    pub remove_len: usize,
    pub replace_target: String,
    pub replace_with: String,
    /// Replace only in the name, not in its extension (Arkha's addition).
    pub replace_keep_ext: bool,
    /// Regex tab (Arkha's addition): the compiled pattern (None = off),
    /// its replacement ($1, ${name}, $0, $$), and leaving the extension out.
    pub regex: Option<regex::Regex>,
    pub regex_with: String,
    pub regex_keep_ext: bool,
    pub digits: usize,
    pub start: u64,
    /// Arkha's additions: put the number at the end instead of in front,
    /// and there before the extension.
    pub number_at_end: bool,
    pub number_before_ext: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            case: Case::Unchanged,
            insert_text: String::new(),
            insert_pos: 0,
            insert_from_end: false,
            insert_before_ext: false,
            remove_from: 0,
            remove_len: 0,
            replace_target: String::new(),
            replace_with: String::new(),
            replace_keep_ext: false,
            regex: None,
            regex_with: String::new(),
            regex_keep_ext: false,
            digits: 0,
            start: 1,
            number_at_end: false,
            number_before_ext: false,
        }
    }
}

/// The new name for the item at `row` (0-based, list order). Like
/// caja-rename it works on the whole name, extension included, in the order
/// case, insert, remove, replace, regex, then numbering.
#[cfg(test)]
fn new_name(name: &str, row: usize, o: &Options) -> String {
    new_name_for(name, false, row, o)
}

/// The extension of `name` ("" for folders, hidden names like ".bashrc", or
/// names without a dot): the last dot and what follows it.
fn ext_start(name: &str, is_dir: bool) -> usize {
    match name.rfind('.') {
        Some(i) if i > 0 && !is_dir => i,
        _ => name.len(),
    }
}

/// `new_name` for an item that may be a folder (folders have no extension).
pub fn new_name_for(name: &str, is_dir: bool, row: usize, o: &Options) -> String {
    let mut s = match o.case {
        Case::Upper => name.to_uppercase(),
        Case::Lower => name.to_lowercase(),
        Case::Title => title_case(name),
        Case::Unchanged => name.to_string(),
    };
    if !o.insert_text.is_empty() {
        let chars: Vec<char> = s.chars().collect();
        // The part Position counts in: the whole name, or the name before
        // its extension; counted from its start or back from its end.
        let limit = if o.insert_before_ext { s[..ext_start(&s, is_dir)].chars().count() } else { chars.len() };
        let pos = o.insert_pos.min(limit);
        let at = if o.insert_from_end { limit - pos } else { pos };
        s = chars[..at].iter().collect::<String>() + &o.insert_text + &chars[at..].iter().collect::<String>();
    }
    if o.remove_len > 0 {
        let chars: Vec<char> = s.chars().collect();
        let len = chars.len() as i64;
        // A negative From counts from the end of the name.
        let from = if o.remove_from >= 0 { o.remove_from as i64 } else { (len + o.remove_from as i64).max(0) };
        let from = from.min(len) as usize;
        let to = (from + o.remove_len).min(chars.len());
        s = chars[..from].iter().chain(&chars[to..]).collect();
    }
    if !(o.replace_target.is_empty() && o.replace_with.is_empty()) {
        // With "exclude extensions", only the part before it is searched.
        let ext = if o.replace_keep_ext { s.split_off(ext_start(&s, is_dir)) } else { String::new() };
        s = if o.replace_target.is_empty() {
            // Like GLib's g_string_replace: an empty search string matches
            // at every position, before, between and after the characters.
            let mut out = o.replace_with.clone();
            for c in s.chars() {
                out.push(c);
                out.push_str(&o.replace_with);
            }
            out
        } else {
            s.replace(&o.replace_target, &o.replace_with)
        } + &ext;
    }
    if let Some(re) = &o.regex {
        let ext = if o.regex_keep_ext { s.split_off(ext_start(&s, is_dir)) } else { String::new() };
        s = re.replace_all(&s, o.regex_with.as_str()).into_owned() + &ext;
    }
    if o.digits > 0 {
        let number = format!("{:0width$}", o.start + row as u64, width = o.digits);
        let at = match (o.number_at_end, o.number_before_ext) {
            (false, _) => 0,
            (true, false) => s.len(),
            (true, true) => ext_start(&s, is_dir),
        };
        s.insert_str(at, &number);
    }
    s
}

const SMALL_WORDS: &[&str] = &["a", "an", "and", "as", "at", "but", "by", "en", "for", "if", "in", "of", "on", "or", "the", "to", "v", "v.", "via", "vs", "vs."];

/// Whether `name` can be a new name: a name, not a path (a "/" from
/// search and replace moved the file into another folder, even one up).
fn name_ok(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/')
}

/// Title case after John Gruber's rules, as in caja-rename's titlecase.h:
/// small words stay lower case except first and last (and after a colon),
/// words with inner capitals or dots ("iPhone", "example.com") stay as they
/// are, all-caps input is lowered first, and parts joined by "-" or "/" are
/// capitalized separately.
pub fn title_case(text: &str) -> String {
    let has_lower = text.chars().any(|c| c.is_lowercase());
    let has_upper = text.chars().any(|c| c.is_uppercase());
    let text = if has_upper && !has_lower { text.to_lowercase() } else { text.to_string() };
    let words: Vec<&str> = text.split(' ').collect();
    let real: Vec<usize> = (0..words.len()).filter(|&i| !words[i].is_empty()).collect();
    let (first, last) = (real.first().copied(), real.last().copied());
    let mut after_colon = false;
    let mut out = Vec::with_capacity(words.len());
    for (i, w) in words.iter().enumerate() {
        if w.is_empty() {
            out.push(String::new());
            continue;
        }
        let edge = Some(i) == first || Some(i) == last || after_colon;
        out.push(title_word(w, edge));
        after_colon = w.ends_with(':');
    }
    out.join(" ")
}

fn title_word(word: &str, edge: bool) -> String {
    // Leave alone: inner capitals ("iPhone") or dots between letters ("example.com").
    let chars: Vec<char> = word.chars().collect();
    let inner_cap = chars.iter().skip(1).any(|c| c.is_uppercase());
    let inner_dot = chars.windows(3).any(|w| w[0].is_alphanumeric() && w[1] == '.' && w[2].is_alphanumeric());
    if inner_cap || inner_dot {
        return word.to_string();
    }
    let core: String = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.').to_lowercase();
    if !edge && SMALL_WORDS.contains(&core.as_str()) {
        return word.to_lowercase();
    }
    // Capitalize each part of hyphenated or slashed words.
    let mut out = String::with_capacity(word.len());
    let mut at_start = true;
    for c in word.chars() {
        if at_start && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            at_start = false;
        } else {
            out.push(c);
            if c == '-' || c == '/' {
                at_start = true;
            } else if c.is_alphanumeric() {
                at_start = false;
            }
        }
    }
    out
}

/// One selected item in the dialog's list.
struct Entry {
    folder: PathBuf,
    old: String,
    is_dir: bool,
}

struct Ui {
    case: [gtk::RadioButton; 4],
    insert_text: gtk::Entry,
    insert_pos: gtk::SpinButton,
    insert_from_end: gtk::CheckButton,
    insert_before_ext: gtk::CheckButton,
    remove_from: gtk::SpinButton,
    remove_len: gtk::SpinButton,
    replace_target: gtk::Entry,
    replace_with: gtk::Entry,
    replace_keep_ext: gtk::CheckButton,
    regex_pattern: gtk::Entry,
    regex_with: gtk::Entry,
    regex_ignore_case: gtk::CheckButton,
    regex_keep_ext: gtk::CheckButton,
    regex_error: gtk::Label,
    digits: gtk::SpinButton,
    start: gtk::SpinButton,
    number_at_end: gtk::CheckButton,
    number_before_ext: gtk::CheckButton,
}

impl Ui {
    fn options(&self) -> Options {
        let case = [Case::Upper, Case::Lower, Case::Title, Case::Unchanged];
        Options {
            case: self.case.iter().zip(case).find(|(b, _)| b.is_active()).map(|(_, c)| c).unwrap_or_default(),
            insert_text: self.insert_text.text().to_string(),
            insert_pos: self.insert_pos.value_as_int().max(0) as usize,
            insert_from_end: self.insert_from_end.is_active(),
            insert_before_ext: self.insert_before_ext.is_active(),
            remove_from: self.remove_from.value_as_int(),
            remove_len: self.remove_len.value_as_int().max(0) as usize,
            replace_target: self.replace_target.text().to_string(),
            replace_with: self.replace_with.text().to_string(),
            replace_keep_ext: self.replace_keep_ext.is_active(),
            regex: self.compile_regex(),
            regex_with: self.regex_with.text().to_string(),
            regex_keep_ext: self.regex_keep_ext.is_active(),
            digits: self.digits.value_as_int().max(0) as usize,
            start: self.start.value().max(0.0) as u64,
            number_at_end: self.number_at_end.is_active(),
            number_before_ext: self.number_before_ext.is_active(),
        }
    }

    /// The Regex tab's pattern, or None if it's empty or invalid (then the
    /// field turns red and the error is shown under it).
    fn compile_regex(&self) -> Option<regex::Regex> {
        let pattern = self.regex_pattern.text();
        let ctx = self.regex_pattern.style_context();
        if pattern.is_empty() {
            ctx.remove_class("error");
            self.regex_error.set_text("");
            self.regex_error.hide();
            return None;
        }
        match regex::RegexBuilder::new(&pattern).case_insensitive(self.regex_ignore_case.is_active()).build() {
            Ok(re) => {
                ctx.remove_class("error");
                self.regex_error.set_text("");
                self.regex_error.hide();
                Some(re)
            }
            Err(e) => {
                ctx.add_class("error");
                // The last line of the message says what's wrong.
                let msg = e.to_string();
                let why = msg.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("").trim().trim_start_matches("error: ").to_string();
                self.regex_error.set_text(&fill(&tr("Invalid regular expression: {why}"), &[("why", &why)]));
                self.regex_error.show();
                None
            }
        }
    }

    fn reset(&self) {
        let d = Options::default();
        self.case[3].set_active(true);
        self.insert_text.set_text(&d.insert_text);
        self.insert_pos.set_value(d.insert_pos as f64);
        self.remove_from.set_value(d.remove_from as f64);
        self.remove_len.set_value(d.remove_len as f64);
        self.replace_target.set_text(&d.replace_target);
        self.replace_with.set_text(&d.replace_with);
        self.regex_pattern.set_text("");
        self.regex_with.set_text(&d.regex_with);
        self.digits.set_value(d.digits as f64);
        self.start.set_value(d.start as f64);
    }
}

/// `/` and `\` can't be part of a name, so they can't be typed either.
fn no_slashes(e: &gtk::Entry) {
    e.connect_insert_text(|e, text, _| {
        if text.contains(['/', '\\']) {
            e.stop_signal_emission_by_name("insert-text");
            let clean: String = text.chars().filter(|c| *c != '/' && *c != '\\').collect();
            if !clean.is_empty() {
                let mut pos = e.position();
                e.insert_text(&clean, &mut pos);
                e.set_position(pos);
            }
        }
    });
}

fn spin(lower: f64, upper: f64, value: f64) -> gtk::SpinButton {
    let s = gtk::SpinButton::with_range(lower, upper, 1.0);
    s.set_value(value);
    s.set_hexpand(true);
    s
}

/// A tab with labelled controls side by side, each label above its control.
fn tab(fields: &[(&str, &gtk::Widget)]) -> gtk::Grid {
    let g = gtk::Grid::new();
    g.set_border_width(12);
    g.set_row_spacing(6);
    g.set_column_spacing(12);
    g.set_column_homogeneous(true);
    for (i, (label, w)) in fields.iter().enumerate() {
        let l = gtk::Label::new(Some(&tr(label)));
        l.set_xalign(0.0);
        g.attach(&l, i as i32, 0, 1, 1);
        g.attach(*w, i as i32, 1, 1, 1);
    }
    g
}

fn message(parent: &gtk::Dialog, kind: gtk::MessageType, buttons: gtk::ButtonsType, title: &str, text: &str) -> gtk::ResponseType {
    let m = gtk::MessageDialog::new(Some(parent), gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT, kind, buttons, text);
    m.set_title(title);
    let r = m.run();
    m.close();
    r
}

/// Shows the Rename All dialog for these items (local files and folders).
pub fn show(parent: &gtk::Window, files: Vec<gio::File>) {
    let entries: Rc<RefCell<Vec<Entry>>> = Rc::new(RefCell::new(
        files
            .iter()
            .filter_map(|f| {
                let path = f.path()?;
                Some(Entry { folder: path.parent()?.to_path_buf(), old: path.file_name()?.to_string_lossy().to_string(), is_dir: path.is_dir() })
            })
            .collect(),
    ));
    let d = gtk::Dialog::new();
    d.set_title(&tr("Rename..."));
    // Kept above its window, but not blocking it (Caja's does): the
    // folders can be looked at and used meanwhile.
    d.set_transient_for(Some(parent));
    d.set_destroy_with_parent(true);
    d.set_position(gtk::WindowPosition::CenterOnParent);
    d.set_default_size(640, 480);
    d.set_type_hint(gdk::WindowTypeHint::Dialog);

    let radios: [gtk::RadioButton; 4] = {
        let up = gtk::RadioButton::with_label(&tr("Upper case"));
        let low = gtk::RadioButton::with_label_from_widget(&up, &tr("Lower case"));
        let title = gtk::RadioButton::with_label_from_widget(&up, &tr("Title case"));
        let same = gtk::RadioButton::with_label_from_widget(&up, &tr("Unchanged"));
        same.set_active(true);
        [up, low, title, same]
    };
    let ui = Rc::new(Ui {
        case: radios,
        insert_text: gtk::Entry::new(),
        insert_pos: spin(0.0, 100.0, 0.0),
        insert_from_end: gtk::CheckButton::with_label(&tr("From the end")),
        insert_before_ext: gtk::CheckButton::with_label(&tr("Before the extension")),
        remove_from: spin(-100.0, 100.0, 0.0),
        remove_len: spin(0.0, 100.0, 0.0),
        replace_target: gtk::Entry::new(),
        replace_with: gtk::Entry::new(),
        replace_keep_ext: gtk::CheckButton::with_label(&tr("Exclude file extensions")),
        regex_pattern: gtk::Entry::new(),
        regex_with: gtk::Entry::new(),
        regex_ignore_case: gtk::CheckButton::with_label(&tr("Ignore case")),
        regex_keep_ext: gtk::CheckButton::with_label(&tr("Exclude file extensions")),
        regex_error: gtk::Label::new(None),
        digits: spin(0.0, 10.0, 0.0),
        start: spin(0.0, 1_000_000_000.0, 1.0),
        number_at_end: gtk::CheckButton::with_label(&tr("At the end")),
        number_before_ext: gtk::CheckButton::with_label(&tr("Before the extension")),
    });
    ui.insert_text.set_max_length(128);
    // Not the regex pattern: it needs "\" for escapes.
    for e in [&ui.insert_text, &ui.replace_target, &ui.replace_with, &ui.regex_with] {
        no_slashes(e);
    }

    let notebook = gtk::Notebook::new();
    // (id, page, label), added below in the saved order.
    let mut pages: Vec<(&str, gtk::Widget, &str)> = vec![];
    let case_grid = gtk::Grid::new();
    case_grid.set_border_width(12);
    case_grid.set_column_spacing(12);
    // One option per row, stacked like caja-rename's.
    for (i, r) in ui.case.iter().enumerate() {
        case_grid.attach(r, 0, i as i32, 1, 1);
    }
    pages.push(("case", case_grid.upcast(), n_("Case")));
    // These checkboxes (and Replace's) aren't reset by Apply: they're ways
    // of working, not edits.
    let insert_tab = tab(&[(n_("Text to insert"), ui.insert_text.upcast_ref()), (n_("Position"), ui.insert_pos.upcast_ref())]);
    ui.insert_from_end.set_tooltip_text(Some(&tr("Count the position back from the end of the name instead of from its start")));
    ui.insert_before_ext.set_tooltip_text(Some(&tr("Count the position only in the name before its extension (e.g. “.jpg”), so the text goes before it. With “From the end”, 0 puts it right before the extension")));
    insert_tab.attach(&ui.insert_from_end, 0, 2, 1, 1);
    insert_tab.attach(&ui.insert_before_ext, 1, 2, 1, 1);
    pages.push(("insert", insert_tab.upcast(), n_("Insert")));
    pages.push(("remove", tab(&[(n_("From"), ui.remove_from.upcast_ref()), (n_("Length"), ui.remove_len.upcast_ref())]).upcast(), n_("Remove")));
    let replace_tab = tab(&[(n_("Text to search for"), ui.replace_target.upcast_ref()), (n_("Text to insert"), ui.replace_with.upcast_ref())]);
    ui.replace_keep_ext.set_tooltip_text(Some(&tr("Search and replace only in the name, not in its extension (e.g. “.jpg”)")));
    replace_tab.attach(&ui.replace_keep_ext, 0, 2, 2, 1);
    pages.push(("replace", replace_tab.upcast(), n_("Replace")));
    let regex_tab = tab(&[(n_("Regular expression"), ui.regex_pattern.upcast_ref()), (n_("Replace with"), ui.regex_with.upcast_ref())]);
    // TRANSLATORS: an example regular expression; keep the expression itself.
    ui.regex_pattern.set_placeholder_text(Some(&tr("e.g. (\\d+)-(\\d+)")));
    ui.regex_with.set_placeholder_text(Some(&tr("e.g. $2-$1")));
    ui.regex_keep_ext.set_tooltip_text(Some(&tr("Match only in the name, not in its extension (e.g. “.jpg”)")));
    regex_tab.attach(&ui.regex_ignore_case, 0, 2, 1, 1);
    regex_tab.attach(&ui.regex_keep_ext, 1, 2, 1, 1);
    // Only takes up room while there's an error to show.
    ui.regex_error.set_no_show_all(true);
    ui.regex_error.set_xalign(0.0);
    ui.regex_error.set_line_wrap(true);
    ui.regex_error.style_context().add_class("error");
    regex_tab.attach(&ui.regex_error, 0, 3, 2, 1);
    // TRANSLATORS: keep $1, $2, ${name}, $0 and $$ as they are.
    let regex_hint = gtk::Label::new(Some(&tr("Every match is replaced. In “Replace with”, $1, $2… or ${name} insert what a group matched, $0 the whole match, $$ a “$”.")));
    regex_hint.set_xalign(0.0);
    regex_hint.set_line_wrap(true);
    regex_hint.style_context().add_class("dim-label");
    regex_tab.attach(&regex_hint, 0, 4, 2, 1);
    pages.push(("regex", regex_tab.upcast(), n_("Regex")));
    let number_tab = tab(&[(n_("Digits"), ui.digits.upcast_ref()), (n_("Start"), ui.start.upcast_ref())]);
    ui.number_at_end.set_tooltip_text(Some(&tr("Put the number after the name instead of in front")));
    ui.number_before_ext.set_tooltip_text(Some(&tr("With “At the end”: put the number before the extension (e.g. “.jpg”) instead of after it")));
    // In front, the number is before the extension anyway.
    ui.number_before_ext.set_sensitive(false);
    {
        let before = ui.number_before_ext.clone();
        ui.number_at_end.connect_toggled(move |c| before.set_sensitive(c.is_active()));
    }
    number_tab.attach(&ui.number_at_end, 0, 2, 1, 1);
    number_tab.attach(&ui.number_before_ext, 1, 2, 1, 1);
    pages.push(("enumerate", number_tab.upcast(), n_("Enumerate")));
    // The tabs can be dragged into any order, which is remembered. Each
    // tab's content sits in a wrapper so that only the current tab's is
    // shown: a notebook is as tall as its tallest page, and this way the
    // tab area fits the current tab and the file list gets the rest.
    for (id, page, label) in &pages {
        let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);
        wrap.add(page);
        wrap.set_widget_name(id);
        notebook.append_page(&wrap, Some(&gtk::Label::new(Some(&tr(label)))));
    }
    crate::util::order_tabs(&notebook, config::get().rename_tab_order, |order| config::update(|s| s.rename_tab_order = order));
    let show_only = |nb: &gtk::Notebook, current: &gtk::Widget| {
        for i in 0..nb.n_pages() {
            let Some(wrap) = nb.nth_page(Some(i)).and_then(|w| w.downcast::<gtk::Box>().ok()) else { continue };
            for content in wrap.children() {
                content.set_visible(wrap.upcast_ref::<gtk::Widget>() == current);
            }
        }
    };
    notebook.connect_switch_page(move |nb, page, _| show_only(nb, page));

    // icon, old name, new name
    let store = gtk::ListStore::new(&[gdk_pixbuf::Pixbuf::static_type(), String::static_type(), String::static_type()]);
    for e in entries.borrow().iter() {
        let icon = gio::File::for_path(e.folder.join(&e.old))
            .query_info("standard::icon", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
            .ok()
            .and_then(|i| i.icon());
        let pb = icons::icon_pixbuf(icon.as_ref(), 22);
        store.insert_with_values(None, &[(0, &pb), (1, &e.old), (2, &e.old)]);
    }
    let tv = gtk::TreeView::with_model(&store);
    if config::with(|s| s.rename_striped_rows) {
        tv.connect_local("draw", true, |vals| {
            let tv = vals[0].get::<gtk::TreeView>().ok()?;
            let cr = vals[1].get::<gtk::cairo::Context>().ok()?;
            crate::pane::draw_stripes(&tv, &cr);
            Some(false.to_value())
        });
    }
    tv.set_headers_visible(false);
    tv.set_show_expanders(false);
    tv.selection().set_mode(gtk::SelectionMode::None);
    let col = |r: &gtk::CellRenderer, attr: &str, i: i32, expand: bool| {
        let c = gtk::TreeViewColumn::new();
        CellLayoutExt::pack_start(&c, r, true);
        CellLayoutExt::add_attribute(&c, r, attr, i);
        c.set_expand(expand);
        tv.append_column(&c);
    };
    col(gtk::CellRendererPixbuf::new().upcast_ref(), "pixbuf", 0, false);
    // Old and new name share the width, as in caja-rename.
    col(gtk::CellRendererText::new().upcast_ref(), "text", 1, true);
    col(gtk::CellRendererText::new().upcast_ref(), "text", 2, true);
    let sw = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    sw.set_shadow_type(gtk::ShadowType::In);
    sw.set_vexpand(true);
    sw.add(&tv);

    // Recompute every new name from the current names and the settings.
    let update = {
        let (store, entries, ui) = (store, entries.clone(), ui.clone());
        Rc::new(move || {
            let o = ui.options();
            for (row, e) in entries.borrow().iter().enumerate() {
                if let Some(it) = store.iter_nth_child(None, row as i32) {
                    store.set(&it, &[(1, &e.old), (2, &new_name_for(&e.old, e.is_dir, row, &o))]);
                }
            }
        })
    };
    for r in &ui.case {
        let u = update.clone();
        r.connect_toggled(move |_| u());
    }
    for e in [&ui.insert_text, &ui.replace_target, &ui.replace_with, &ui.regex_pattern, &ui.regex_with] {
        let u = update.clone();
        e.connect_changed(move |_| u());
    }
    {
        let u = update.clone();
        ui.replace_keep_ext.connect_toggled(move |_| u());
    }
    for c in [&ui.insert_from_end, &ui.insert_before_ext, &ui.number_at_end, &ui.number_before_ext, &ui.regex_ignore_case, &ui.regex_keep_ext] {
        let u = update.clone();
        c.connect_toggled(move |_| u());
    }
    for s in [&ui.insert_pos, &ui.remove_from, &ui.remove_len, &ui.digits, &ui.start] {
        let u = update.clone();
        s.connect_value_changed(move |_| u());
    }

    let close = gtk::Button::with_mnemonic(&tr("_Close"));
    close.set_image(Some(&gtk::Image::from_icon_name(Some("dialog-close"), gtk::IconSize::Button)));
    close.set_always_show_image(true);
    let apply = gtk::Button::with_mnemonic(&tr("_Apply"));
    apply.set_image(Some(&gtk::Image::from_icon_name(Some("dialog-ok"), gtk::IconSize::Button)));
    apply.set_always_show_image(true);
    {
        let d = d.clone();
        close.connect_clicked(move |_| d.response(gtk::ResponseType::Close));
    }
    {
        let (d, entries, ui, update) = (d.clone(), entries, ui, update);
        apply.connect_clicked(move |_| {
            let o = ui.options();
            let plan: Vec<(usize, String)> = entries.borrow().iter().enumerate().map(|(i, e)| (i, new_name_for(&e.old, e.is_dir, i, &o))).collect();
            let line = |i: usize, new: &str| format!("{} -> {}", entries.borrow()[i].old, new);
            // 1. Names that can't be used.
            let bad: Vec<String> = plan.iter().filter(|(_, n)| !name_ok(n)).map(|(i, n)| line(*i, n)).collect();
            if !bad.is_empty() {
                message(&d, gtk::MessageType::Warning, gtk::ButtonsType::Close, &tr("Invalid names"), &fill(&tr("The following names are not acceptable:\n\n{names}"), &[("names", &bad.join("\n"))]));
                return;
            }
            // 2. Existing files that would be replaced.
            let changed: Vec<&(usize, String)> = plan.iter().filter(|(i, n)| *n != entries.borrow()[*i].old).collect();
            let clash: Vec<String> = changed.iter().filter(|(i, n)| entries.borrow()[*i].folder.join(n).exists()).map(|(i, n)| line(*i, n)).collect();
            if !clash.is_empty() {
                let r = message(&d, gtk::MessageType::Question, gtk::ButtonsType::YesNo, &tr("Confirm overwrite"), &fill(&tr("The following will be overwritten:\n\n{names}\n\nDo you wish to continue?"), &[("names", &clash.join("\n"))]));
                if r != gtk::ResponseType::Yes {
                    return;
                }
            }
            // 3. Rename, in list order (one step for Edit → Undo).
            let mut failed = vec![];
            let mut done = vec![];
            for (i, n) in &changed {
                let (folder, old) = {
                    let e = &entries.borrow()[*i];
                    (e.folder.clone(), e.old.clone())
                };
                // Through GIO, which takes the item's notes, emblems and
                // custom icon along (GVfs keeps them by path; renamed past
                // it, they were lost). Replacing what's there was asked.
                let (from, to) = (gio::File::for_path(folder.join(&old)), gio::File::for_path(folder.join(n)));
                let flags = gio::FileCopyFlags::OVERWRITE | gio::FileCopyFlags::NOFOLLOW_SYMLINKS | gio::FileCopyFlags::NO_FALLBACK_FOR_MOVE;
                match from.move_(&to, flags, gio::Cancellable::NONE, None) {
                    Ok(()) => {
                        done.push((gio::File::for_path(folder.join(&old)).uri().to_string(), gio::File::for_path(folder.join(n)).uri().to_string()));
                        entries.borrow_mut()[*i].old = n.clone();
                    }
                    Err(_) => failed.push(format!("{old} -> {n}")),
                }
            }
            crate::ops::note_renames(done);
            if !failed.is_empty() {
                message(&d, gtk::MessageType::Error, gtk::ButtonsType::Close, &tr("Rename error"), &fill(&tr("There were errors while renaming the following:\n\n{names}"), &[("names", &failed.join("\n"))]));
            }
            // 4. Start over from the new names; the dialog stays open.
            ui.reset();
            update();
        });
    }
    let buttons = gtk::ButtonBox::new(gtk::Orientation::Horizontal);
    buttons.set_layout(gtk::ButtonBoxStyle::End);
    buttons.set_spacing(6);
    buttons.add(&close);
    buttons.add(&apply);

    let bx = d.content_area();
    bx.set_spacing(6);
    bx.set_border_width(6);
    bx.pack_start(&notebook, false, false, 0);
    bx.pack_start(&sw, true, true, 0);
    bx.pack_start(&buttons, false, false, 0);
    d.show_all();
    // show_all() showed every tab's content; keep only the current one.
    if let Some(cur) = notebook.nth_page(notebook.current_page()) {
        show_only(&notebook, &cur);
    }
    // Close, or the window's own close button.
    // SAFETY: the dialog is ours; nothing uses it once it's closed.
    d.connect_response(|d, _| unsafe { d.destroy() });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_names_stay_names() {
        assert!(name_ok("Holiday 01.jpg"));
        for bad in ["", ".", "..", "a/b.jpg", "../up.jpg", "/abs"] {
            assert!(!name_ok(bad), "{bad}");
        }
    }


    fn o() -> Options {
        Options::default()
    }

    #[test]
    fn steps_and_order() {
        assert_eq!(new_name("Photo One.JPG", 0, &o()), "Photo One.JPG");
        assert_eq!(new_name("Photo One.JPG", 0, &Options { case: Case::Lower, ..o() }), "photo one.jpg");
        assert_eq!(new_name("abc", 0, &Options { case: Case::Upper, ..o() }), "ABC");
        // Insert counts characters from the start; past the end appends.
        assert_eq!(new_name("äbc.txt", 0, &Options { insert_text: "X".into(), insert_pos: 1, ..o() }), "äXbc.txt");
        assert_eq!(new_name("ab", 0, &Options { insert_text: "X".into(), insert_pos: 50, ..o() }), "abX");
        // Remove: from the start, or (negative From) from the end.
        assert_eq!(new_name("abcdef", 0, &Options { remove_from: 1, remove_len: 2, ..o() }), "adef");
        assert_eq!(new_name("img.jpeg", 0, &Options { remove_from: -4, remove_len: 4, ..o() }), "img.");
        assert_eq!(new_name("ab", 0, &Options { remove_from: -10, remove_len: 1, ..o() }), "b");
        // Replace: literal, every occurrence, case-sensitive.
        assert_eq!(new_name("a_b_A", 0, &Options { replace_target: "_".into(), replace_with: " ".into(), ..o() }), "a b A");
        assert_eq!(new_name("a.b", 0, &Options { replace_target: ".".into(), replace_with: "".into(), ..o() }), "ab");
        assert_eq!(new_name("ab", 0, &Options { replace_with: "-".into(), ..o() }), "-a-b-");
        // Excluding extensions: only the part before the last dot changes.
        let keep = Options { replace_target: "jpg".into(), replace_with: "photo".into(), replace_keep_ext: true, ..o() };
        assert_eq!(new_name("jpg_1.jpg", 0, &keep), "photo_1.jpg");
        assert_eq!(new_name("a.b.jpg", 0, &Options { replace_target: ".".into(), replace_with: "_".into(), replace_keep_ext: true, ..o() }), "a_b.jpg");
        assert_eq!(new_name(".jpg", 0, &keep), ".photo"); // hidden name: no extension
        assert_eq!(new_name_for("jpg.jpg", true, 0, &keep), "photo.photo"); // folders have none
        assert_eq!(new_name("jpg", 0, &keep), "photo");
        // Order: case, insert, remove, replace, then the number in front.
        let all = Options { case: Case::Upper, insert_text: "x".into(), insert_pos: 0, remove_from: 1, remove_len: 1, replace_target: "B".into(), replace_with: "_".into(), digits: 3, start: 9, ..o() };
        assert_eq!(new_name("abc", 0, &all), "009x_C");
        assert_eq!(new_name("abc", 2, &all), "011x_C");
        assert_eq!(new_name("f", 0, &Options { digits: 1, start: 123, ..o() }), "123f");
        // Insert: "From the end" and "Before the extension", alone and together.
        let ins = Options { insert_text: "_x".into(), ..o() };
        assert_eq!(new_name("photo.jpg", 0, &Options { insert_from_end: true, ..ins.clone() }), "photo.jpg_x");
        assert_eq!(new_name("photo.jpg", 0, &Options { insert_from_end: true, insert_pos: 4, ..ins.clone() }), "photo_x.jpg");
        assert_eq!(new_name("photo.jpg", 0, &Options { insert_before_ext: true, insert_pos: 2, ..ins.clone() }), "ph_xoto.jpg");
        assert_eq!(new_name("photo.jpg", 0, &Options { insert_before_ext: true, insert_pos: 50, ..ins.clone() }), "photo_x.jpg");
        let both = Options { insert_from_end: true, insert_before_ext: true, ..ins };
        assert_eq!(new_name("photo.jpg", 0, &both), "photo_x.jpg");
        assert_eq!(new_name("photo.jpg", 0, &Options { insert_pos: 2, ..both.clone() }), "pho_xto.jpg");
        assert_eq!(new_name("ab.jpg", 0, &Options { insert_pos: 9, ..both.clone() }), "_xab.jpg");
        assert_eq!(new_name(".bashrc", 0, &both), ".bashrc_x");
        assert_eq!(new_name_for("my.folder", true, 0, &both), "my.folder_x");
        // Enumerate: in front, at the very end, or at the end before the extension.
        let num = Options { digits: 2, ..o() };
        assert_eq!(new_name("photo.jpg", 0, &Options { number_at_end: true, ..num.clone() }), "photo.jpg01");
        assert_eq!(new_name("photo.jpg", 4, &Options { number_at_end: true, number_before_ext: true, ..num.clone() }), "photo05.jpg");
        assert_eq!(new_name("photo.jpg", 0, &Options { number_before_ext: true, ..num.clone() }), "01photo.jpg");
        assert_eq!(new_name_for("dir.d", true, 0, &Options { number_at_end: true, number_before_ext: true, ..num }), "dir.d01");
        assert_eq!(new_name("a.tar.gz", 0, &Options { insert_text: "-v2".into(), digits: 2, number_at_end: true, number_before_ext: true, ..both }), "a.tar-v201.gz");
    }

    #[test]
    fn regex_tab() {
        let re = |p: &str| Some(regex::Regex::new(p).unwrap());
        let swap = Options { regex: re(r"(\d+)-(\d+)"), regex_with: "$2-$1".into(), ..o() };
        assert_eq!(new_name("img 12-34.jpg", 0, &swap), "img 34-12.jpg");
        // Every match; named groups; the whole match.
        assert_eq!(new_name("a1b22c333", 0, &Options { regex: re(r"\d+"), regex_with: "#".into(), ..o() }), "a#b#c#");
        assert_eq!(new_name("2024_photo.jpg", 0, &Options { regex: re(r"^(?P<y>\d{4})_(?P<n>\w+)"), regex_with: "${n}_${y}".into(), ..o() }), "photo_2024.jpg");
        assert_eq!(new_name("ab", 0, &Options { regex: re("b"), regex_with: "[$0]".into(), ..o() }), "a[b]");
        // Ignore case is part of the compiled pattern.
        let ci = regex::RegexBuilder::new("JPG").case_insensitive(true).build().ok();
        assert_eq!(new_name("x.jpg", 0, &Options { regex: ci, regex_with: "png".into(), ..o() }), "x.png");
        // Leaving the extension out.
        let keep = Options { regex: re("jpg"), regex_with: "pic".into(), regex_keep_ext: true, ..o() };
        assert_eq!(new_name("jpg.jpg", 0, &keep), "pic.jpg");
        // Runs after Replace and before numbering.
        let order = Options { replace_target: "a".into(), replace_with: "b".into(), regex: re("b+"), regex_with: "c".into(), digits: 1, ..o() };
        assert_eq!(new_name("aab", 0, &order), "1c");
    }

    #[test]
    fn title_case_rules() {
        assert_eq!(title_case("the lord of the rings"), "The Lord of the Rings");
        assert_eq!(title_case("THE WIND IN THE WILLOWS"), "The Wind in the Willows");
        assert_eq!(title_case("my iPhone and example.com"), "My iPhone and example.com");
        assert_eq!(title_case("step-by-step guide"), "Step-By-Step Guide");
        assert_eq!(title_case("notes: a list"), "Notes: A List");
        assert_eq!(title_case("holiday photo.jpg"), "Holiday photo.jpg");
    }

    #[test]
    fn random_rename_options() {
        let patterns = [r"\d+", "a", "^(.)(.*)$", r"(?P<x>\w)", "", ".", r"\s"];
        crate::testutil::cases(3000, |r| {
            let name = r.text(16);
            assert_eq!(new_name(&name, 0, &o()), name);
            let opts = Options {
                case: *r.pick(&[Case::Upper, Case::Lower, Case::Title, Case::Unchanged]),
                insert_text: r.text(4),
                insert_pos: r.below(30),
                insert_from_end: r.chance(2),
                insert_before_ext: r.chance(2),
                remove_from: r.int(-30, 30) as i32,
                remove_len: r.below(30),
                replace_target: r.text(2),
                replace_with: r.text(3),
                replace_keep_ext: r.chance(2),
                regex: if r.chance(2) { regex::Regex::new(r.pick(&patterns)).ok() } else { None },
                regex_with: r.text(4),
                regex_keep_ext: r.chance(2),
                digits: r.below(5),
                start: r.below(1000) as u64,
                number_at_end: r.chance(2),
                number_before_ext: r.chance(2),
            };
            let _ = new_name_for(&name, r.chance(3), r.below(100), &opts);
            let _ = title_case(&name);
        });
    }
}
