#!/bin/bash
# archive-manager.sh x11|wayland: Extract Here and Compress use the archive
# manager chosen in Preferences → Behavior → Archive manager, with its own
# command line (Ark: batch extraction next to the archive, Compress with
# its dialog; Engrampa: --extract-here); "Automatic" takes the one set for
# archives, else the first installed, Ark first on KDE. Stand-ins named
# ark and engrampa, first on PATH, note how they're called.
GT_A11Y=1 GT_CONF='default_view=list
archiver=ark'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

S=$HOME/src; mkdir -p "$S" "$OUT/bin"
echo x > "$S/x.txt"; (cd "$S" && zip -q test.zip x.txt); rm "$S/x.txt"
CALLS=$OUT/calls.log
for a in ark engrampa; do printf '#!/bin/sh\necho "%s $*" >> "%s"\n' "$a" "$CALLS" > "$OUT/bin/$a"; chmod +x "$OUT/bin/$a"; done
export PATH=$OUT/bin:$PATH
last() { sleep 1.5; tail -n1 "$CALLS" 2>/dev/null | sed "s|$S/||g"; }
select_zip() { gt_click 300 147; sleep 0.3; gt_click 300 147; sleep 0.3; }

gt_display
gt_arkha "$S"
select_zip
gt_act extract
gt_check "Ark chosen: Extract Here" "$(last)" "ark --batch --autodestination --autosubfolder test.zip"
gt_act compress
gt_check "Ark chosen: Compress, with its dialog" "$(last)" "ark --changetofirstpath --add --dialog test.zip"
gt_quit

printf '[General]\ndefault_view=list\narchiver=engrampa\n' > "$XDG_CONFIG_HOME/arkha/arkha.conf"
gt_arkha "$S"; select_zip
gt_act extract
gt_check "Engrampa chosen: Extract Here" "$(last)" "engrampa --extract-here test.zip"
gt_quit

# Automatic, on KDE, nothing set for archives: Ark.
printf '[General]\ndefault_view=list\narchiver=\n' > "$XDG_CONFIG_HOME/arkha/arkha.conf"
XDG_CURRENT_DESKTOP=KDE gt_arkha "$S"; select_zip
gt_act extract
gt_check "Automatic on KDE: Ark" "$(last)" "ark --batch --autodestination --autosubfolder test.zip"

# The choice in Preferences, on its Behavior page.
gdbus call --session --dest $APP_ID --object-path $APP_PATH/window/1 \
  --method org.gtk.Actions.Activate preferences [] {} >/dev/null & sleep 2
y=180; [ "$BACKEND" = wayland ] && y=194
gt_click 240 $y; gt_key Down; sleep 0.5
# Scrolled down to the Archives section, for the screenshot.
xdotool mousemove 600 400 click 5 click 5 click 5 click 5; sleep 0.5
gt_shot behavior
gt_check "Preferences offers the choice" "$(gt_a11y labels | grep -c '^Archive manager:$')" "1"
gt_end
