#!/bin/sh
# Build and install Arkha for the current user (~/.local).
# Usage: ./install.sh            install
#        ./install.sh --default  install and make it the default folder handler
set -e
cd "$(dirname "$0")"
cargo build --release
# Nothing installed without the manual (its loop below would pass without it).
test -f target/release/manual/en/index.html
PREFIX="${PREFIX:-$HOME/.local}"
install -Dm755 target/release/arkha "$PREFIX/bin/arkha"
install -Dm644 arkha.desktop "$PREFIX/share/applications/arkha.desktop"
install -Dm644 data/arkha.svg "$PREFIX/share/icons/hicolor/scalable/apps/arkha.svg"
install -Dm644 data/io.github.dekart811.Arkha.metainfo.xml "$PREFIX/share/metainfo/io.github.dekart811.Arkha.metainfo.xml"
# The translations the build made from po/*.po (none without gettext).
for mo in target/release/locale/*/LC_MESSAGES/arkha.mo; do
    [ -e "$mo" ] && install -Dm644 "$mo" "$PREFIX/share/locale/${mo#target/release/locale/}"
done
# The manual's pages the build made from docs/manual (gone ones too).
rm -rf "$PREFIX/share/arkha/manual"
find target/release/manual -type f | while read -r f; do
    install -Dm644 "$f" "$PREFIX/share/arkha/${f#target/release/}"
done
# Only where the theme has a cache, which then has to know the new icon.
[ -f "$PREFIX/share/icons/hicolor/icon-theme.cache" ] && gtk-update-icon-cache -q "$PREFIX/share/icons/hicolor" 2>/dev/null || true
update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
if [ "$1" = "--default" ]; then
    # Remember the current folder handler, so uninstall.sh can restore it.
    PREVIOUS="$(xdg-mime query default inode/directory 2>/dev/null || true)"
    if [ -n "$PREVIOUS" ] && [ "$PREVIOUS" != "arkha.desktop" ]; then
        CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/arkha"
        mkdir -p "$CONFIG"
        echo "$PREVIOUS" > "$CONFIG/previous-folder-app"
    fi
    xdg-mime default arkha.desktop inode/directory
    echo "Arkha is now the default application for folders."
fi
echo "Installed to $PREFIX/bin/arkha"
