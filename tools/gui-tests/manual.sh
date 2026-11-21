#!/bin/bash
# manual.sh x11|wayland: Help → Manual and F1 open the manual (its first
# page, built next to the program) in the web browser (the application for
# web links), not the one for HTML files, which on some systems is an
# editor; that one only without a browser. Both are stand-ins here that
# only note what they were asked to open. In German
# (from Preferences) the German manual; in a language it isn't written in,
# the English one. Without the manual installed, its message offers this
# version's manual online, on GitHub.
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

OPENED=$OUT/opened
mkdir -p "$XDG_DATA_HOME/applications"
EDITED=$OUT/edited
# stand_in NAME LOG MIME: an application noting what it opens in LOG.
stand_in() {
  printf '#!/bin/sh\necho "$1" >> "%s"\n' "$2" > "$OUT/$1"
  chmod +x "$OUT/$1"
  printf '[Desktop Entry]\nType=Application\nName=%s\nExec=%s %%u\nMimeType=%s;\n' "$1" "$OUT/$1" "$3" > "$XDG_DATA_HOME/applications/$1.desktop"
}
stand_in notebrowser "$OPENED" x-scheme-handler/https
stand_in noteeditor "$EDITED" text/html
printf '[Default Applications]\ntext/html=noteeditor.desktop\nx-scheme-handler/https=notebrowser.desktop\n' > "$XDG_CONFIG_HOME/mimeapps.list"
update-desktop-database "$XDG_DATA_HOME/applications" 2>/dev/null || true
PAGE="$GT_ROOT/target/release/manual/en/index.html"
# (As a path or a file:// URI, as GIO passes it.)
opened() { sleep 2; tail -n1 "$OPENED" 2>/dev/null | sed 's|^file://||'; }

gt_display
gt_arkha "$HOME"
gt_act manual
gt_check "Help → Manual opens the manual's first page in the browser" "$(opened)" "$PAGE"
gt_check "not in the editor for HTML files" "$(cat "$EDITED" 2>/dev/null)" ""
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
gt_quit

# Not installed: a copy of the program in a bin folder of its own, with no
# share/arkha/manual beside it. Its message offers the manual online: this
# development build's commit, in English (the session's language).
rm -f "$XDG_CONFIG_HOME/arkha/arkha.conf" "$OPENED"
mkdir -p "$OUT/bin" && cp "$ARKHA" "$OUT/bin/arkha"
ARKHA=$OUT/bin/arkha
gt_arkha "$HOME"
gt_act manual; sleep 1
gt_check "not installed: the message says so" "$(gt_a11y labels | grep -c 'The manual isn’t installed')" "1"
gt_ok "Read It Online" gt_a11y press "Read It Online"
ONLINE="https://github.com/dekart811/arkha/blob/$(git -C "$GT_ROOT" rev-parse --short HEAD)/docs/manual/en/index.md"
gt_check "Read It Online opens this version's manual on GitHub" "$(opened)" "$ONLINE"
gt_quit

# No web browser: the application for HTML files.
printf '[Default Applications]\ntext/html=noteeditor.desktop\n' > "$XDG_CONFIG_HOME/mimeapps.list"
rm "$XDG_DATA_HOME/applications/notebrowser.desktop"
update-desktop-database "$XDG_DATA_HOME/applications" 2>/dev/null || true
ARKHA=$GT_ROOT/target/release/arkha
rm -f "$XDG_CONFIG_HOME/arkha/arkha.conf"
gt_arkha "$HOME"
gt_act manual
gt_check "without a browser: the application for HTML files" "$(sleep 2; tail -n1 "$EDITED" 2>/dev/null | sed 's|^file://||')" "$PAGE"
gt_end
