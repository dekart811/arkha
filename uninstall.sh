#!/bin/sh
# Remove what install.sh installed for the current user (~/.local).
# Usage: ./uninstall.sh          remove Arkha, keep its settings
#        ./uninstall.sh --purge  also delete its settings (~/.config/arkha)
# Use the same PREFIX as for installing, if you set one.
set -e
PREFIX="${PREFIX:-$HOME/.local}"
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}"

removed=0
for f in "$PREFIX/bin/arkha" "$PREFIX/share/applications/arkha.desktop" "$PREFIX/share/icons/hicolor/scalable/apps/arkha.svg" "$PREFIX/share/metainfo/io.github.dekart811.Arkha.metainfo.xml"; do
    if [ -e "$f" ]; then
        rm -f "$f"
        echo "Removed $f"
        removed=1
    fi
done
for mo in "$PREFIX"/share/locale/*/LC_MESSAGES/arkha.mo; do
    [ -e "$mo" ] && rm -f "$mo"
done
rm -rf "$PREFIX/share/arkha/manual"
rmdir "$PREFIX/share/arkha" 2>/dev/null || true
[ "$removed" = 1 ] || echo "Arkha isn't installed in $PREFIX."
update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
[ -f "$PREFIX/share/icons/hicolor/icon-theme.cache" ] && gtk-update-icon-cache -q "$PREFIX/share/icons/hicolor" 2>/dev/null || true

# If --default made Arkha the folder handler, give folders back to the
# application it replaced, or else drop just that association so the
# desktop's own default applies again.
MIMEAPPS="$CONFIG/mimeapps.list"
PREVIOUS_FILE="$CONFIG/arkha/previous-folder-app"
if [ -f "$MIMEAPPS" ] && grep -q '^inode/directory=.*arkha\.desktop' "$MIMEAPPS"; then
    PREVIOUS="$(cat "$PREVIOUS_FILE" 2>/dev/null || true)"
    if [ -n "$PREVIOUS" ]; then
        xdg-mime default "$PREVIOUS" inode/directory
        echo "Folders open with $PREVIOUS again."
    else
        sed -i -e '/^inode\/directory=/ s/arkha\.desktop;\{0,1\}//' -e '/^inode\/directory=$/d' "$MIMEAPPS"
        echo "Arkha is no longer the default application for folders."
    fi
fi
rm -f "$PREVIOUS_FILE"

# Only removes the folder if nothing else is in it.
rmdir "$CONFIG/arkha" 2>/dev/null || true

if [ "$1" = "--purge" ]; then
    if [ -d "$CONFIG/arkha" ]; then
        rm -rf "$CONFIG/arkha"
        echo "Deleted the settings in $CONFIG/arkha"
    fi
else
    [ -d "$CONFIG/arkha" ] && echo "Kept the settings in $CONFIG/arkha (./uninstall.sh --purge deletes them)."
fi
exit 0
