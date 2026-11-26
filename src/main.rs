//! Arkha — a Caja-style file manager in Rust + GTK 3.

mod a11y;
mod actions;
mod arkdnd;
mod autorun;
mod bulkrename;
mod checksums;
mod clipboard;
mod columns;
mod config;
mod connect;
mod details;
mod iconlabel;
mod iconview;
mod info;
mod fm1;
mod folders;
mod i18n;
mod icons;
mod imagesize;
mod keys;
mod metadata;
mod backgrounds;
mod ops;
mod pane;
mod props;
mod scroll;
mod search;
mod sidebar;
#[cfg(test)]
mod testutil;
mod util;
mod thumbcache;
mod thumbnailers;
mod window;
mod xds;

use gio::prelude::*;
use gtk::prelude::*;

fn main() -> glib::ExitCode {
    // First: it may set LANGUAGE, which threads mustn't see change.
    i18n::init(&config::with(|s| s.language.clone()));
    pane::quiet_header_drags();
    // Before any window: they load icons (see there).
    util::tell_metadata_store_in_background();
    let app = gtk::Application::new(
        // The same as the AppStream metainfo's (data/), its D-Bus name.
        Some("io.github.dekart811.Arkha"),
        gio::ApplicationFlags::HANDLES_OPEN,
    );
    // -v, --version: the version, then done, before any window or a running
    // Arkha is asked to open one.
    app.add_main_option(
        "version",
        glib::Char::from(b'v'),
        glib::OptionFlags::NONE,
        glib::OptionArg::None,
        &i18n::tr("Show Arkha's version and quit"),
        None,
    );
    // TRANSLATORS: in --help's first line, after "arkha [OPTION…]".
    app.set_option_context_parameter_string(Some(&i18n::tr("[FOLDER…]")));
    app.connect_handle_local_options(|_, options| {
        if options.contains("version") {
            println!("Arkha {}", util::version());
            return 0;
        }
        -1
    });
    app.connect_startup(|app| {
        let quit = gio::SimpleAction::new("quit", None);
        let a = app.clone();
        quit.connect_activate(move |_, _| {
            if ops::is_busy() {
                // Let running operations finish; just close the windows, and
                // show the operations, whose button goes with them.
                ops::open_window();
                for w in window::all_windows() {
                    w.window.close();
                }
                return;
            }
            a.quit();
        });
        app.add_action(&quit);
        gtk::Window::set_default_icon_name(util::app_icon());
        scroll::init();
        a11y::fix_defunct_states();
        util::set_iec_units(config::with(|s| s.iec_units));
        util::set_date_format(&config::with(|s| s.date_format.clone()));
        sidebar::init_automount();
        // "Show in folder" for other programs.
        fm1::init();
        // Started by D-Bus for that (--gapplication-service): staying a
        // moment for the call. (Otherwise Arkha quits with its last window.)
        if app.flags().contains(gio::ApplicationFlags::IS_SERVICE) {
            app.set_inactivity_timeout(10_000);
        }
        // For tests: what inserting a medium does, with a folder for it.
        if let Some(dir) = std::env::var_os("ARKHA_AUTORUN_TRY") {
            let f = gio::File::for_path(dir);
            glib::timeout_add_local_once(std::time::Duration::from_secs(1), move || autorun::try_folder(&f));
        }
    });
    app.connect_activate(|app| {
        window::AppWindow::new(app, &[]);
    });
    app.connect_open(|app, files, _| {
        let dirs: Vec<gio::File> = files
            .iter()
            .map(|f| {
                let is_dir = f
                    .query_info("standard::type", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
                    .map(|i| i.file_type() == gio::FileType::Directory)
                    .unwrap_or(true);
                if is_dir { f.clone() } else { f.parent().unwrap_or_else(|| f.clone()) }
            })
            .collect();
        window::AppWindow::new(app, &dirs);
    });
    app.run()
}
