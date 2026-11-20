#!/bin/bash
# props-drop.sh x11|wayland: items dragged into an open Properties window
# are shown in it, on the tab chosen before if they have it. A video's
# Media Info, then another video dragged in (its Media Info at once), a
# folder (no Media Info: Basic), and the first video again (Media Info:
# the tab chosen was kept). A note still being typed when another item is
# dropped in is saved first.
GT_A11Y=1 GT_CONF='default_view=list'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

V=$HOME/videos; mkdir -p "$V/sub"
ffmpeg -loglevel error -f lavfi -i testsrc=duration=2:size=320x240:rate=15 "$V/a.mp4"
ffmpeg -loglevel error -f lavfi -i testsrc=duration=4:size=320x240:rate=15 "$V/b.mp4"
gt_display
gt_arkha "$V"
# The list: sub, a.mp4, b.mp4 (folders first); their icons stay
# beside the Properties window, which opens in the middle.
row() { case $1 in sub) echo 147 ;; a) echo 172 ;; b) echo 197 ;; esac; }
drop() {  # drop ROW: dragged from the list into the Properties window
  local y; y=$(row "$1")
  # Selected first: pressed on a row not selected, a drag draws a
  # selection rectangle instead (as in Caja). The first click only takes
  # the focus from the Properties window (xmonad's clickJustFocuses), the
  # second, apart from it (not a double-click), selects.
  gt_click 210 "$y"; sleep 0.3; gt_click 210 "$y"; gt_shot "pick-$1"
  xdotool mousemove 210 "$y" mousedown 1; sleep 0.3
  for i in $(seq 1 12); do xdotool mousemove $((210 + i * 32)) $((y + i * (400 - y) / 12)); sleep 0.05; done
  xdotool mouseup 1; sleep 2
}
title() { gt_a11y dialogs | grep -c "^$1 Properties\$"; }
duration() { gt_a11y labels | grep -E '^[0-9]+ s [0-9]+ ms$' | head -1; }
gt_click 210 "$(row a)"; gt_act properties; sleep 1.5
gt_ok "Media Info chosen" gt_a11y tab "Media Info"; sleep 2
gt_check "a.mp4: its Media Info" "$(title a.mp4) $(duration)" "1 2 s 0 ms"
drop b; gt_shot b
gt_check "b.mp4 dropped in: its Media Info, at once" "$(title b.mp4) $(duration)" "1 4 s 0 ms"
drop sub; gt_shot sub
gt_check "the folder dropped in: Basic (it has no Media Info)" "$(title sub) $(duration)|" "1 |"
drop a; gt_shot a
gt_check "a.mp4 again: Media Info, the tab chosen" "$(title a.mp4) $(duration)" "1 2 s 0 ms"
gt_check "still one Properties window" "$(gt_a11y dialogs | grep -c 'Properties$')" "1"
# A note being typed (saved a moment after typing stops) when another
# item is dropped in: saved, to the item it was typed for.
gt_ok "Notes chosen" gt_a11y tab Notes; sleep 0.5
# Typed into the note (clicked: the focus is the list's, after the drops).
gt_click 600 400; sleep 0.3; gt_click 600 400; gt_type "typed just before"
drop b
note() { gio info -a metadata::annotation "$1" 2>/dev/null | sed -n 's/^ *metadata::annotation: //p'; }
gt_check "the note is saved to a.mp4, b.mp4 shown (on Notes)" "$(note "$V/a.mp4") | $(note "$V/b.mp4") | $(title b.mp4)" "typed just before |  | 1"
gt_end
