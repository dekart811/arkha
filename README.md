# <img src="data/arkha.svg" alt="" height="32" align="absmiddle"> Arkha

A fast graphical file manager for Linux, modelled on Caja (MATE). It uses your desktop's GTK theme, icons and bookmarks, and feels at home on MATE, but works on any desktop.

<!-- Light or dark, as the reader's GitHub is; made with Adwaita and Papirus. -->
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/icons-dark.png">
  <img src="docs/screenshots/icons-light.png" alt="Arkha's icon view, with the info panel showing the selected picture">
</picture>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/split-dark.png">
  <img src="docs/screenshots/split-light.png" alt="Split view: a home folder in the icon view and an album in the list view, each with tabs">
</picture>

<p>
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/rename-dark.png">
  <img src="docs/screenshots/rename-light.png" alt="Rename All replacing IMG_ with Holiday in eight file names" width="49%">
</picture>
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/checksums-dark.png">
  <img src="docs/screenshots/checksums-light.png" alt="The Checksums tab of a picture's properties" width="49%">
</picture>
</p>

## Highlights

- **Familiar:** menus, toolbar, path bar, side pane and tabs as in Caja, and notes, emblems, custom icons and bookmarks shared with it.
- **Four views:** icons, list, compact and columns, with the list's columns and sort order your choice.
- **Split view:** two panes side by side, each with its own tabs, swapped with Ctrl+U.
- **Tabs like a browser:** reopen closed tabs, drag them between windows or out into a new one.
- **Info panel:** a folder's cover picture, an image or a file's preview beside the files.
- **Rename many files at once:** change case, insert, remove, search and replace (also with regular expressions), number them.
- **Safe file operations:** undo and redo, and copies checked against their originals when it matters.
- **Checksums** (MD5, SHA-1, SHA-256, CRC32 and more) in a file's properties.
- **Your own context menu entries and thumbnailers,** and every keyboard shortcut changeable.
- **In English and German,** following your system's language or the one you choose.
- **Light and quick:** it opens big folders with less CPU and memory than Caja and Nemo ([measured](#speed)).

The [full list of features](docs/FEATURES.md) has every detail, and [the manual](docs/manual/en/index.md) ([auf Deutsch](docs/manual/de/index.md)) explains how to use them (in Arkha: Help → Manual, F1).

## Speed

Arkha opens big folders sooner and with less memory than Caja and Nemo, and scrolls through them with less work:

| | Arkha 1.3.0 | Caja 1.28 | Nemo 6.8 |
|---|---|---|---|
| Opening `/usr/lib` (4,594 items) | **1.39 s** | 1.83 s | 2.46 s |
| Opening a folder of 20,000 files | **2.14 s** | 2.67 s | 3.13 s |
| Memory with the 20,000 files open | **182 MB** | 232 MB | 231 MB |
| CPU time for scrolling through them | **0.39 s** | 0.69 s | 2.52 s |

It also makes thumbnails only for the files in sight, not for the whole folder at once.

Each program opened each folder three times in the same private, headless X server, on the same machine (8 virtual CPUs); the table shows the medians. A folder counts as open once the program's main thread goes idle. Measured on 25 November 2026 with [`tools/bench/compare.sh`](tools/bench/compare.sh), which you can run yourself: results differ from machine to machine, but the order shouldn't.

## Installing

Packages for each release are on the project's Releases page, and for Arch Linux also in the AUR:

- **Debian 13 or newer, Ubuntu 26.04 or newer:** `sudo apt install ./arkha_*.deb`
- **Fedora:** `sudo dnf install ./arkha-*.rpm`
- **Arch Linux:** `sudo pacman -U ./arkha-*.pkg.tar.zst`, or from the AUR: `arkha` (built from the release's sources), `arkha-bin` (prebuilt) or `arkha-git` (the newest development version).

To try the newest development version without building it: the same four packages are kept for master's latest three successful [CI runs](https://github.com/dekart811/arkha/actions/workflows/ci.yml?query=branch%3Amaster+is%3Asuccess). Open one and download from its **Artifacts** (GitHub asks you to sign in); each file is named with the commit it was built from. They're builds of work in progress, not releases.

Arkha uses GVfs for the Trash, network places and drives; most desktops have it already. To build it yourself, see [BUILDING.md](BUILDING.md). If you package Arkha for a distribution, build it from a release's git tag, never from the prebuilt files: see [Packaging Arkha for a distribution](BUILDING.md#packaging-arkha-for-a-distribution).

Your settings are kept in `~/.config/arkha/`.

## Translations

Arkha speaks English and German so far. A new language is one file to translate; [CONTRIBUTING.md](CONTRIBUTING.md#translating) explains how.

## Contributing

Bug reports, translations and code are welcome: see [CONTRIBUTING.md](CONTRIBUTING.md).

## Written by an AI

Arkha's code, its German translation and its documentation were written by an AI, Anthropic's Claude (in Claude Code), not by a human programmer. Its maintainer decided what Arkha should do and how it should behave, and tried out and checked the results.

## License

Arkha is free software under the [GNU General Public License, version 3 or later](LICENSE). Some of its texts and the search's groups of file types come from [Caja](https://github.com/mate-desktop/caja) (GPL-2.0-or-later), by the MATE developers.
