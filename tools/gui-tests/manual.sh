#!/bin/bash
# manual.sh x11|wayland: Help → Manual and F1 open the manual (its first
# page, built next to the program) in the application for HTML files: here
# a stand-in "browser" that only notes what it was asked to open. In German
# (from Preferences) the German manual; in a language it isn't written in,
# the English one.
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

OPENED=$OUT/opened
mkdir -p "$XDG_DATA_HOME/applications"
printf '#!/bin/sh\necho "$1" >> "%s"\n' "$OPENED" > "$OUT/notebrowser"
chmod +x "$OUT/notebrowser"
cat > "$XDG_DATA_HOME/applications/notebrowser.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Note Browser
Exec=$OUT/notebrowser %u
MimeType=text/html;
EOF
printf '[Default Applications]\ntext/html=notebrowser.desktop\n' > "$XDG_CONFIG_HOME/mimeapps.list"
update-desktop-database "$XDG_DATA_HOME/applications" 2>/dev/null || true
PAGE="$GT_ROOT/target/release/manual/en/index.html"
# (As a path or a file:// URI, as GIO passes it.)
opened() { sleep 2; tail -n1 "$OPENED" 2>/dev/null | sed 's|^file://||'; }

gt_display
gt_arkha "$HOME"
gt_act manual
gt_check "Help → Manual opens the manual's first page" "$(opened)" "$PAGE"
rm -f "$OPENED"
gt_key F1
gt_check "F1 opens it too" "$(opened)" "$PAGE"
gt_quit

# German chosen in Preferences: the German manual.
printf '[General]\nlanguage=de\n' > "$XDG_CONFIG_HOME/arkha/arkha.conf"
rm -f "$OPENED"
gt_arkha "$HOME"
gt_act manual
gt_check "in German: the German manual" "$(opened)" "${PAGE%/en/index.html}/de/index.html"
gt_quit

# A language the manual isn't written in (French): the English one.
printf '[General]\nlanguage=fr\n' > "$XDG_CONFIG_HOME/arkha/arkha.conf"
rm -f "$OPENED"
gt_arkha "$HOME"
gt_act manual
gt_check "in French: the English manual" "$(opened)" "$PAGE"
gt_end
