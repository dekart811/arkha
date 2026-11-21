//! File → Connect to Server…, as Caja's: a server, its type (SSH, FTP,
//! Windows share, WebDAV), and optionally a share, port, folder, user name
//! and domain, made into an address (sftp://user@server:2222/folder) that
//! the window opens; the pane mounts it (see Pane::mount_and_load), and
//! GTK's dialog asks for a password if the server wants one, so Arkha never
//! handles passwords itself. Optionally bookmarked under a name.

use std::rc::Rc;

use gtk::prelude::*;

use crate::i18n::{n_, tr};
use crate::util;

/// The kinds of server: (name, URI scheme, default port, has a user name).
const KINDS: [(&str, &str, u16, bool); 6] = [
    (n_("SSH"), "sftp", 22, true),
    (n_("Public FTP"), "ftp", 21, false),
    (n_("FTP (with login)"), "ftp", 21, true),
    (n_("Windows share"), "smb", 445, true),
    (n_("WebDAV (HTTP)"), "dav", 80, true),
    (n_("Secure WebDAV (HTTPS)"), "davs", 443, true),
];
/// KINDS' Windows share, the one with a share and a domain.
const SMB: usize = 3;
/// KINDS' public FTP, logged into as "anonymous".
const PUBLIC_FTP: usize = 1;

/// What the dialog's fields say.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct Server {
    pub kind: usize,
    pub host: String,
    /// 0 for the kind's own.
    pub port: u16,
    pub share: String,
    pub folder: String,
    pub user: String,
    pub domain: String,
}

impl Server {
    /// The address to open: scheme://[domain;][user@]host[:port]/[share/]folder,
    /// with the parts escaped. None without a server. A whole address typed
    /// as the server ("sftp://…") is taken as it is.
    pub fn uri(&self) -> Option<String> {
        let host = self.host.trim();
        if host.is_empty() {
            return None;
        }
        if host.contains("://") {
            return Some(host.to_string());
        }
        let (_, scheme, _, has_user) = KINDS[self.kind.min(KINDS.len() - 1)];
        let esc = |s: &str| glib::Uri::escape_string(s, None, false).to_string();
        let user = self.user.trim();
        let mut userinfo = String::new();
        if self.kind == PUBLIC_FTP {
            userinfo.push_str("anonymous@");
        } else if has_user && !user.is_empty() {
            let domain = self.domain.trim();
            if self.kind == SMB && !domain.is_empty() {
                userinfo.push_str(&esc(domain));
                userinfo.push(';');
            }
            userinfo.push_str(&esc(user));
            userinfo.push('@');
        }
        // An IPv6 address goes in brackets.
        let host = if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host.to_string() };
        let port = if self.port == 0 { String::new() } else { format!(":{}", self.port) };
        let mut parts: Vec<&str> = vec![];
        if self.kind == SMB {
            parts.extend(self.share.split('/'));
        }
        parts.extend(self.folder.split('/'));
        let path: Vec<String> = parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).map(esc).collect();
        Some(format!("{scheme}://{userinfo}{host}{port}/{}", path.join("/")))
    }
}

/// Shows the dialog; `open` gets the address to open once Connect is pressed
/// (after it's bookmarked, if asked for).
pub fn show(parent: &gtk::Window, open: impl Fn(String) + 'static) {
    let d = gtk::Dialog::with_buttons(
        Some(&tr("Connect to Server")),
        Some(parent),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        &[(tr("_Cancel").as_str(), gtk::ResponseType::Cancel), (tr("C_onnect").as_str(), gtk::ResponseType::Ok)],
    );
    d.set_default_response(gtk::ResponseType::Ok);
    d.set_resizable(false);
    let grid = gtk::Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(12);
    grid.set_border_width(12);

    let heading = |text: &str| {
        let l = gtk::Label::new(None);
        l.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(text)));
        l.set_xalign(0.0);
        l
    };
    let entry = || {
        let e = gtk::Entry::new();
        e.set_activates_default(true);
        e.set_hexpand(true);
        e.set_width_chars(28);
        e
    };
    let server = entry();
    let kind = gtk::ComboBoxText::new();
    for (name, ..) in KINDS {
        kind.append_text(&tr(name));
    }
    kind.set_active(Some(0));
    let share = entry();
    let port = entry();
    port.set_input_purpose(gtk::InputPurpose::Digits);
    port.set_width_chars(6);
    port.set_hexpand(false);
    port.set_halign(gtk::Align::Start);
    let folder = entry();
    let domain = entry();
    let user = entry();
    let bookmark = gtk::CheckButton::with_mnemonic(&tr("Add _bookmark"));
    let bookmark_name = entry();
    bookmark_name.set_sensitive(false);

    // A row per field: its label (whose mnemonic focuses the field) and the field.
    let row = std::cell::Cell::new(0);
    let rows: std::cell::RefCell<Vec<(gtk::Label, gtk::Widget)>> = Default::default();
    let next = || {
        let r = row.get();
        row.set(r + 1);
        r
    };
    let add = |label: &str, w: &gtk::Widget| {
        let l = gtk::Label::with_mnemonic(&tr(label));
        l.set_xalign(1.0);
        l.set_mnemonic_widget(Some(w));
        let r = next();
        grid.attach(&l, 0, r, 1, 1);
        grid.attach(w, 1, r, 1, 1);
        rows.borrow_mut().push((l, w.clone()));
    };
    grid.attach(&heading(&tr("Server Details")), 0, next(), 2, 1);
    add(n_("_Server:"), server.upcast_ref());
    add(n_("_Type:"), kind.upcast_ref());
    add(n_("Sh_are:"), share.upcast_ref());
    add(n_("_Port:"), port.upcast_ref());
    add(n_("_Folder:"), folder.upcast_ref());
    let user_heading = heading(&tr("User Details"));
    grid.attach(&user_heading, 0, next(), 2, 1);
    add(n_("_Domain name:"), domain.upcast_ref());
    add(n_("_User name:"), user.upcast_ref());
    grid.attach(&bookmark, 0, next(), 2, 1);
    add(n_("Bookmark _name:"), bookmark_name.upcast_ref());
    let rows = rows.into_inner();
    let rows = Rc::new(rows);
    d.content_area().add(&grid);

    let read = {
        let (server, kind, share, port, folder, user, domain) =
            (server.clone(), kind.clone(), share.clone(), port.clone(), folder.clone(), user.clone(), domain.clone());
        move || Server {
            kind: kind.active().unwrap_or(0) as usize,
            host: server.text().to_string(),
            port: port.text().trim().parse().unwrap_or(0),
            share: share.text().to_string(),
            folder: folder.text().to_string(),
            user: user.text().to_string(),
            domain: domain.text().to_string(),
        }
    };
    let read = Rc::new(read);
    // Only the fields the kind of server has; Connect once there's a server.
    let update = {
        let (d, rows, user_heading, port, read) = (d.clone(), rows, user_heading, port.clone(), read.clone());
        let (share, domain, user) = (share.clone(), domain.clone(), user.clone());
        move || {
            let s = read();
            let (_, _, default_port, has_user) = KINDS[s.kind.min(KINDS.len() - 1)];
            port.set_placeholder_text(Some(&default_port.to_string()));
            for (l, w) in rows.iter() {
                let shown = if w == share.upcast_ref::<gtk::Widget>() || w == domain.upcast_ref::<gtk::Widget>() {
                    s.kind == SMB
                } else if w == user.upcast_ref::<gtk::Widget>() {
                    has_user
                } else {
                    true
                };
                l.set_visible(shown);
                w.set_visible(shown);
            }
            user_heading.set_visible(has_user);
            d.set_response_sensitive(gtk::ResponseType::Ok, s.uri().is_some());
        }
    };
    let update = Rc::new(update);
    for e in [&server, &share, &port, &folder, &user, &domain] {
        let u = update.clone();
        e.connect_changed(move |_| u());
    }
    {
        let u = update.clone();
        kind.connect_changed(move |_| u());
    }
    {
        let name = bookmark_name.clone();
        bookmark.connect_toggled(move |b| name.set_sensitive(b.is_active()));
    }
    grid.show_all();
    update();
    server.grab_focus();

    let open = Rc::new(open);
    d.connect_response(move |d, r| {
        if r == gtk::ResponseType::Ok {
            let Some(uri) = read().uri() else { return };
            if bookmark.is_active() {
                if let Err(e) = util::bookmark_named(&uri, &bookmark_name.text()) {
                    util::error_dialog(Some(d.upcast_ref()), &tr("Could not add bookmark"), &e.to_string());
                    return;
                }
            }
            open(uri);
        }
        d.close();
    });
    d.show();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(kind: usize, host: &str) -> Server {
        Server { kind, host: host.into(), ..Default::default() }
    }

    #[test]
    fn addresses() {
        assert_eq!(server(0, "").uri(), None);
        assert_eq!(server(0, "  ").uri(), None);
        assert_eq!(server(0, "example.org").uri().as_deref(), Some("sftp://example.org/"));
        let s = Server { port: 2222, folder: "/srv/data/".into(), user: "anna".into(), ..server(0, " example.org ") };
        assert_eq!(s.uri().as_deref(), Some("sftp://anna@example.org:2222/srv/data"));
        // Public FTP logs in as anonymous, whatever the user name says.
        let s = Server { user: "anna".into(), ..server(PUBLIC_FTP, "ftp.example.org") };
        assert_eq!(s.uri().as_deref(), Some("ftp://anonymous@ftp.example.org/"));
        assert_eq!(Server { user: "anna".into(), ..server(2, "ftp.example.org") }.uri().as_deref(), Some("ftp://anna@ftp.example.org/"));
        assert_eq!(server(4, "dav.example.org").uri().as_deref(), Some("dav://dav.example.org/"));
        assert_eq!(server(5, "dav.example.org").uri().as_deref(), Some("davs://dav.example.org/"));
    }

    #[test]
    fn windows_shares() {
        let s = Server { share: "Public".into(), folder: "Photos 2026".into(), user: "anna".into(), domain: "HOME".into(), ..server(SMB, "nas") };
        assert_eq!(s.uri().as_deref(), Some("smb://HOME;anna@nas/Public/Photos%202026"));
        // The share and domain only count for Windows shares.
        let s = Server { share: "Public".into(), domain: "HOME".into(), user: "anna".into(), ..server(0, "nas") };
        assert_eq!(s.uri().as_deref(), Some("sftp://anna@nas/"));
        // Without a share: the server's shares.
        assert_eq!(server(SMB, "nas").uri().as_deref(), Some("smb://nas/"));
    }

    #[test]
    fn escaping() {
        let s = Server { user: "a@b c".into(), folder: "x/ä ö/#1".into(), ..server(0, "h") };
        assert_eq!(s.uri().as_deref(), Some("sftp://a%40b%20c@h/x/%C3%A4%20%C3%B6/%231"));
        // An IPv6 address in brackets, once.
        assert_eq!(server(0, "fe80::1").uri().as_deref(), Some("sftp://[fe80::1]/"));
        assert_eq!(server(0, "[fe80::1]").uri().as_deref(), Some("sftp://[fe80::1]/"));
        // A whole address typed as the server is taken as it is.
        assert_eq!(server(SMB, "sftp://other/x").uri().as_deref(), Some("sftp://other/x"));
    }
}
