#!/bin/sh
# build.sh: builds Arkha's .deb, on Debian 13 or newer (or a relative such
# as Ubuntu 26.04): run in a copy of the sources made by build-on.sh (the
# crates in vendor/, built without internet; the version in VERSION), or
# in a checkout with internet for the crates. Needs cargo and rustc 1.85+,
# libgtk-3-dev, gettext, pkg-config, build-essential and dpkg-dev. The
# package goes to target/deb/.
set -e
cd "$(dirname "$0")/../.."
top=$(pwd)
version=$(sh packaging/version.sh deb)
arch=$(dpkg --print-architecture)
offline=
[ -d vendor ] && offline=--offline
cargo build --release --locked $offline

out=$top/target/deb
stage=$out/arkha_${version}-1_${arch}
rm -rf "$out"
mkdir -p "$stage/DEBIAN"
install -Dm755 target/release/arkha "$stage/usr/bin/arkha"
install -Dm644 arkha.desktop "$stage/usr/share/applications/arkha.desktop"
install -Dm644 data/arkha.svg "$stage/usr/share/icons/hicolor/scalable/apps/arkha.svg"
install -Dm644 data/io.github.dekart811.Arkha.metainfo.xml "$stage/usr/share/metainfo/io.github.dekart811.Arkha.metainfo.xml"
# The translations build.rs made from po/*.po.
for mo in target/release/locale/*/LC_MESSAGES/arkha.mo; do
    [ -e "$mo" ] && install -Dm644 "$mo" "$stage/usr/share/locale/${mo#target/release/locale/}"
done
# The manual's pages build.rs made from docs/manual.
# Never a package without them (the loop below would pass without any).
test -f target/release/manual/en/index.html
find target/release/manual -type f | while read -r f; do
    install -Dm644 "$f" "$stage/usr/share/arkha/${f#target/release/}"
done
install -Dm644 README.md "$stage/usr/share/doc/arkha/README.md"
# Debian keeps the GPL's text in base-files' /usr/share/common-licenses.
cat > "$stage/usr/share/doc/arkha/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: Arkha
Source: https://github.com/dekart811/arkha

Files: *
Copyright: 2026 dekart811
License: GPL-3.0+
Comment: Some texts and the search's groups of file types come from Caja
 (GPL-2.0-or-later), by the MATE developers.

License: GPL-3.0+
 This program is free software: you can redistribute it and/or modify it
 under the terms of the GNU General Public License as published by the Free
 Software Foundation, either version 3 of the License, or (at your option)
 any later version.
 .
 On Debian systems, the complete text of the GNU General Public License
 version 3 can be found in /usr/share/common-licenses/GPL-3.
EOF

# The libraries it's linked with, by their Debian package names and the
# versions it needs (dpkg-shlibdeps wants a debian/control to run in).
mkdir -p "$out/debian"
printf 'Source: arkha\n' > "$out/debian/control"
depends=$(cd "$out" && dpkg-shlibdeps -O -e "$stage/usr/bin/arkha" 2>/dev/null | sed -n 's/^shlibs:Depends=//p')
[ -n "$depends" ] || { echo "dpkg-shlibdeps found no dependencies" >&2; exit 1; }

# The menu entry's and the icon's caches are updated by dpkg's triggers
# (desktop-file-utils, hicolor-icon-theme): no maintainer scripts needed.
cat > "$stage/DEBIAN/control" <<EOF
Package: arkha
Version: ${version}-1
Architecture: ${arch}
Maintainer: ${DEB_MAINTAINER:-dekart811 <mail@dekart811.net>}
Installed-Size: $(du -sk --exclude=DEBIAN "$stage" | cut -f1)
Depends: ${depends}
Recommends: gvfs, bubblewrap
Suggests: gvfs-backends, ffmpegthumbnailer | ffmpeg, mediainfo, engrampa | file-roller | xarchiver | ark
Section: utils
Priority: optional
Description: fast graphical file manager
 Arkha is a file manager for GTK desktops: icon, list, compact and columns
 views, tabs and split view, an info panel, bulk renaming, checksums,
 checked copies, custom actions and thumbnailers, and undo for file
 operations. It uses the desktop's GTK theme, icons and bookmarks, and
 shares notes, emblems and custom icons with Caja.
EOF
dpkg-deb --root-owner-group -Zxz --build "$stage" "$out/" >/dev/null
ls "$out"/*.deb
