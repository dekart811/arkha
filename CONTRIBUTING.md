# Contributing to Arkha

Bug reports, translations and code are welcome. This file has the technical side: testing, translating and how the code is laid out; building Arkha and its packages is described in [BUILDING.md](BUILDING.md). What you contribute is published under Arkha's license, the [GPL, version 3 or later](LICENSE).

## Reporting a bug

Say what you did, what you expected and what happened instead, and include the version from Help → About (for example "1.0.0-dev (r143, 87938a8)"), your distribution and your desktop (and whether it runs on X11 or Wayland). Messages Arkha prints when started from a terminal help too.

## Testing

Every push to `master` and every pull request is checked on GitHub (`.github/workflows/ci.yml`): the build, the unit tests, clippy (no warnings), the translations and their placeholders, and, once those have passed, a package for every distribution Arkha supports, each built and installed in a container of that distribution as the release makes it: Debian 13 (with its Rust 1.85, the oldest Arkha supports), Ubuntu 26.04 (the Debian package installed there, as the release offers it for Ubuntu too), Fedora and Arch Linux, and the prebuilt program for the others. A commit that doesn't need it (a typo in a document, say) can skip the checks with `[skip ci]` in its message; never tag one for a release, as GitHub then skips the release workflow too.

```sh
cargo test --release                                  # unit tests (no display needed)
cargo test                                            # the same as a debug build, as CI runs them (also checks what's handed to GLib)
ARKHA_RANDOM_CASES=50 cargo test --release random     # the randomized tests, 50 times as many inputs
cargo test --release -- --ignored stress              # file operation stress tests (ARKHA_STRESS_FILES=n)
tools/soak.py --backend x11 --minutes 30              # the whole program, driven at random
tools/soak.py --backend wayland --minutes 30 --cycle  # one round repeated: does memory keep growing?
tools/soak.py --backend x11 --minutes 10 --heaptrack   # memory recorded by heaptrack
tools/gui-tests/run-all.sh                            # the GUI tests, X11 and Wayland (cargo build --release first)
tools/gui-tests/undo.sh wayland                       # one of them
tools/bench/compare.sh                                # speed and memory against Caja and Nemo
tools/bench/speed.sh OLD-BINARY target/release/arkha  # opening folders, two builds compared
tools/check-placeholders.py                           # every {place} in a translatable text filled, in the code and the .po files
tools/soak.py --backend x11 --minutes 7 --language de  # a soak run with Arkha in German
tools/screenshots/take.sh                             # the README's screenshots again (light and dark)
```

`tools/soak.py` runs Arkha in a bubblewrap sandbox (the system read-only, `/home` and `/mnt` hidden, its own D-Bus session, a headless X server, and for Wayland Weston inside it) and drives it through its D-Bus actions, keyboard and mouse input (clicks, drags, the wheel, keys, typeahead, inline renames, search; xdotool) and its dialogs through accessibility (AT-SPI), while files keep changing underneath. The sandbox has no applications, so opening a file never starts another program. It reports crashes (with backtraces, as Arkha runs under gdb), GLib/GTK warnings, slow or missing responses, and memory over time, in `target/soak/<run>/`. `--fatal-criticals` stops at the first critical warning for its backtrace, `--fatal-warnings` at the first warning of any kind (GVfs's, gdk-pixbuf's…); `--name` keeps runs of the same seed apart (`target/soak/<backend>-seed<N>-<name>`). Its once-a-minute status line includes the sandbox home's free space: GTK and GVfs warnings about writing files there mean the 1 GiB home has filled up. For test coverage, give it a binary built with `RUSTFLAGS="-C instrument-coverage"` (`--binary`) and `LLVM_PROFILE_FILE=/tmp/soak-run/arkha-%p-%m.profraw`: the profiles land in the run's folder. Needs `bubblewrap`, `xorg-server-xvfb`, `weston`, `gdb`, `xdotool`, `at-spi2-core` and `python-gobject`.

The GUI tests in `tools/gui-tests/` check features with real keyboard and mouse input and read the result back from the files, from Arkha's log, or through accessibility (window titles, tabs and dialogs, the same on X11 and Wayland). Each runs in a private D-Bus session with its own runtime folder, home and settings, in a headless X server with xmonad, or Weston's kiosk shell in it for Wayland, and fails on any GLib/GTK warning in Arkha's log. It prints a PASS or FAIL line per check; screenshots and logs are in `target/gui-tests/<test>-<backend>/`. `lib.sh` holds what they share (writing another test is a few lines of `gt_click`, `gt_key`, `gt_act` and `gt_check`; `gt_go`, `gt_title`, `gt_conf` and `gt_prefs` go to a folder, read the window's title, change a setting and open Preferences), `fixtures.sh` makes what they need in `target/gui-fixtures/` (a `/usr/share` without applications, so nothing else is ever started; an xmonad build; for the benchmarks 20,000 files and sample files of 30 or so types). `tools/lockbar-test.sh` checks Lock Toolbar with real drags. `archive-drag.sh` and `xds-drag.sh` drag an item out of a stand-in archive manager (`fake-archiver.py`, as KDE's Ark or with XDS) in Weston's desktop shell; `xds-drag.sh`'s Wayland run needs XWayland (`xorg-xwayland`).

| Test | What it checks |
|---|---|
| `navbuttons` | Mouse back and forward buttons over the views as set in Preferences (default, swapped, off); right and middle clicks |
| `tabs` | Where new tabs go |
| `undo` | Undo of a copy and a move; Ctrl+Z in the location bar undoes nothing |
| `selection` | After a paste exactly the copies are selected (50 and 1,500 files) |
| `rename` | Renaming in place: Enter, Escape, a click elsewhere, a reload meanwhile |
| `fm1` | "Show in folder" for other programs (org.freedesktop.FileManager1) |
| `metadata-stuck` | Arkha keeps answering while GVfs's metadata service doesn't |
| `prefs` | Every Preferences page; adding and removing a custom action |
| `backgrounds` | Backgrounds and Emblems opens and closes |
| `focuscol` | No accessibility assertion after the columns view hides the cursor's column |
| `swap` | Swapping the panes of split view: the menu, Ctrl+U and the toolbar button |
| `columns` | The list columns: default, one added from a header's menu (and kept), the Trash's own; sorting by permissions |
| `undo-redo` | Undo and Redo of a move to the trash, a new folder named at once, renames |
| `check-copies` | Checked copies started with Paste and Check, Copy To's box and an Alt drop |
| `ops-queue` | File Operations stays open while a queued copy takes over from the one before, until the last job is done (the window check on X11) |
| `bookmarks` | The bookmark editor shows paths, the bookmarks file keeps the URLs |
| `properties` | An emblem ticked and a note typed in Properties, kept in GVfs metadata under Caja's keys |
| `checksums` | The Checksums tab: only the chosen ones, in order; SHA-256 matches sha256sum, the other buttons wait meanwhile; Cancel; Calculate All; Copy |
| `language` | German chosen in the settings: the status bar (with its plural) and side pane in German; the system's language: English again |
| `tab-order` | Properties' and Rename All's tabs dragged into another order, which is kept |
| `props-tabs` | Properties tabs hidden and shown again in Preferences → Properties, and hidden with a right-click on the tab (X11); Basic can't be hidden |
| `previews` | Thumbnails always (with a size limit) or never; folders' item counts always or never |
| `connect` | Connect to Server: the fields for each type, the bookmark, a server that refuses; with `ARKHA_TEST_SFTP=HOST` also a real SFTP mount and the path bar naming it, with `ARKHA_TEST_SMB=USER@HOST/SHARE` the password dialog, cancelled |
| `leave-big` | Leaving a folder of 20,000 files takes well under 2 s, in the list and the icon view (it took 6–7 s) |

`tools/screenshots/take.sh` takes the screenshots in `docs/screenshots/` again, for when Arkha's look changes: Arkha runs headless in a made-up home folder, as in the GUI tests, under Metacity for its title bars, in Adwaita with Papirus (light) and Adwaita-dark with Papirus-Dark (dark), and each window is cut out with its shadow on a transparent background. Besides what the GUI tests need, it needs `metacity`, the Papirus icon themes, the Cantarell font and the `en_US.UTF-8` locale.

`tools/bench/compare.sh` measures what the audits show: opening `/usr/lib` and 20,000 files (when the main thread stays idle, its CPU, all threads'), scrolling, memory and thumbnails, against Caja and Nemo where installed; `speed.sh` compares two builds of Arkha opening folders in the icon view, the list view and the list view with every column. `sample.sh` profiles without perf (gdb samples the main thread's stack), summarized by `sample-report.py`.

## Translating

Arkha's texts are translated with gettext. Each language is a file in `po/` (`po/de.po` is German); the build turns them into catalogs (`build.rs`, into `target/release/locale/`), and Preferences offers each by the name in its `X-Language-Name` header.

- **A new language:** `msginit -i po/arkha.pot -o po/<code>.po -l <code>` (e.g. `fr`), add `"X-Language-Name: <the language's name in itself>\n"` to its header, translate it (by hand or with Poedit, Lokalize, …), check it with `msgfmt --check --statistics -o /dev/null po/<code>.po`, and build.
- **After changing texts in the code:** `tools/update-po.sh` collects them again into `po/arkha.pot` and updates every `.po` file (new texts untranslated, changed ones marked fuzzy).
- **In the code:** a text people see is written `tr("…")`, one with a number in it `trn("{n} file", "{n} files", n)`, a text in a table `n_("…")` (translated where it's shown); values go in named places filled by `fill()` (`fill(&tr("Copying “{name}”"), &[("name", &name)])`), so a translation can order the words its own way. A comment starting `TRANSLATORS:` right above it is shown to translators.
- Keep each `{name}` and the `_` of a menu shortcut (`_File`) in a translation; `msgfmt --check` checks the places. The GUI tests always run in English.

## The manual

The manual (Help → Manual) is written in Markdown, in `docs/manual/en/` (English) and `docs/manual/de/` (German), one file a page, with the same file names in each; GitHub shows it as it is. `build.rs` (with `build/manual.rs`) turns it into HTML pages in `target/release/manual/`, with `docs/manual/style.css`, which the packages install in `/usr/share/arkha/manual/`.

- **A change people will notice** (a new feature, a changed menu, a new setting) goes into the manual in the same commit, in English and German. The German pages name menus, settings and buttons as `po/de.po` translates them.
- **Pages** come in the order `index.md` links to them. Link to another page as to its file, `[Views](views.md)`, or to a heading on it, `views.md#zoom` (GitHub's anchors: lower case, spaces as hyphens, no punctuation). The build warns about links to pages or headings that aren't there, and about links outside the manual (those should be web addresses).
- **Pictures** are the README's screenshots (`docs/screenshots/`, in light and dark: see `window.md` for the `<picture>` to use) or the manual's own, in `docs/manual/images/`.
- **The keyboard shortcuts' tables** in `shortcuts.md` are made from `src/keys.rs` (in German with the actions' names from `po/de.po`): a test fails when they differ, and `ARKHA_WRITE_MANUAL=1 cargo test manual_lists` writes them again.
- **A new language** goes into `docs/manual/<code>/`, with the same file names. Help → Manual opens the pages in Arkha's language when there are any, else the English ones. For its shortcut tables, add the language to the test in `src/keys.rs` (its markers, and its key names in `manual_table`).

## Code layout

| File | What it does |
|---|---|
| `src/main.rs` | GtkApplication setup, command-line paths |
| `src/i18n.rs` | Translations: `tr()`, `trn()`, `n_()`, `fill()`, the language chosen in Preferences |
| `build.rs`, `po/` | The translations (`po/<language>.po`, template `po/arkha.pot`), built into catalogs; `tools/update-po.sh` refreshes them |
| `build/manual.rs`, `docs/manual/` | The manual, from Markdown into HTML pages (see [The manual](#the-manual)) |
| `src/window.rs` | Main window, toolbar, path bar, tabs, all commands |
| `src/window/menus.rs` | The menu bar and the context menus |
| `src/window/prefs.rs` | Preferences, and the editors for menus, places, bookmarks, shortcuts, thumbnailers and custom actions |
| `src/columns.rs` | The side columns of the columns view (folders above, selected folder's contents): clicks, context menus, drag and drop, live updates |
| `src/info.rs` | The info panel: folder covers, image previews and thumbnails, details |
| `src/sidebar.rs` | Side pane: places, devices (mount, eject, automount), bookmarks, section order |
| `src/a11y.rs` | A workaround for GTK 3 accessible objects that report no state once their widget is gone |
| `src/scroll.rs` | Sideways wheel scrolling and middle-click autoscroll in every scrolled area, and a workaround for a GTK 3 overlay-scrollbar bug |
| `src/pane.rs` | One tab: folder loading, monitoring, icon/list views, selection, DnD, inline rename |
| `src/pane/signals.rs` | What clicks, keys, drags and edits do in a tab's views |
| `src/pane/build.rs` | The parts of a tab made on their own: the sorted model and its filter, the list columns, the Trash bar |
| `src/search.rs` | Searching a folder and its subfolders by name |
| `src/bulkrename.rs` | Rename All…: renaming several items at once |
| `src/keys.rs` | Keyboard shortcuts and their editor |
| `src/details.rs` | What can be shown about a file besides its name: the list columns, sort orders and icon captions |
| `src/iconlabel.rs` | The icon view's cells drawn like Caja's: wrapped names cut to three lines, captions, tinted selected icons, boxed selected names |
| `src/iconview.rs` | The icon view: sized in big folders without measuring every item for each size request, and drawing only the rows in sight |
| `src/ops.rs` | Copy / move / link / trash / delete engine on a worker thread, progress + conflict dialogs |
| `src/clipboard.rs` | Caja/Nautilus-compatible file clipboard |
| `src/backgrounds.rs` | Backgrounds and Emblems: folder and side pane backgrounds, emblems by dragging, your own ones |
| `src/metadata.rs` | GVfs metadata (emblems, notes, custom icons, backgrounds) read and written by a thread of its own |
| `src/props.rs` | Properties dialog: basic info and custom icon, emblems, permissions with owner and group, Open With, notes, media info, checksums |
| `src/checksums.rs` | Checksums of a file (MD5, SHA family, CRC32, BLAKE2b): for the Checksums tab and checked copies |
| `src/icons.rs` | Icon cache, background thumbnailer, thumbnail caches |
| `src/imagesize.rs` | An image's width and height from its file header, so big pictures can wait their turn for thumbnails |
| `src/thumbnailers.rs` | Custom thumbnailer commands, and the system's (`.thumbnailer` files) |
| `src/config.rs` | Settings file, zoom levels. A new setting is one line in `settings!` (name, type, default; the key is its name), plus its check in `checked()` if only some values are allowed |
| `src/actions.rs` | Context menu entries (hiding, order), custom actions |
| `src/folders.rs` | View and sorting remembered per folder |
| `src/util.rs` | Formatting, naming, bookmarks, notifications, helpers |
| `src/testutil.rs` | Test helpers: scratch folders, random inputs |
