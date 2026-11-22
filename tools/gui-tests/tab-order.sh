#!/bin/bash
# tab-order.sh x11|wayland: the tabs of Properties and of Rename All can be
# dragged into another order, which is kept: dragged with the real mouse
# (X11), then the window opened again (for Properties also for a folder,
# which hasn't some of a file's tabs: their places are kept, each after the
# tab before it). On Wayland a dialog's place on the screen can't be found
# out, and Alt doesn't get through nested Weston, so there the order is
# given in the settings and checked to be applied the same way.
GT_A11Y=1
[ "$1" = wayland ] && GT_CONF='props_tab_order=notes;media;basic;emblems;permissions;open-with;checksums;
rename_tab_order=regex;case;insert;remove;replace;enumerate;'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

T=$HOME/tabs; mkdir -p "$T/folder"; echo a > "$T/a.txt"; echo b > "$T/b.txt"
gt_display
gt_arkha "$T"
tabs() { gt_a11y dialog-tabs | tr '\n' ' '; }
to_front() {  # to_front TAB FIRST: TAB dragged in front of FIRST (on X11)
  [ "$BACKEND" = wayland ] || drag_tab "$1" "$2"
}
drag_tab() {  # drag_tab FROM TO: FROM dropped on TO's left half
  local from to; from=$(gt_a11y where-tab "$1"); to=$(gt_a11y where-tab "$2")
  [ -n "$from" ] && [ -n "$to" ] || { echo "FAIL no tab $1 or $2" >> "$OUT/result"; return; }
  local fx fy tx; read -r fx fy <<< "$from"; read -r tx _ <<< "$to"
  xdotool mousemove "$fx" "$fy" mousedown 1; sleep 0.3
  for i in $(seq 1 12); do xdotool mousemove $(( fx + (tx - 20 - fx) * i / 12 )) "$fy"; sleep 0.05; done
  sleep 0.3; xdotool mouseup 1; sleep 0.8
}
props() { gt_click 900 650; gt_type "$1"; gt_key Escape; gdbus call --session --dest $APP_ID \
  --object-path $APP_PATH/window/1 --method org.gtk.Actions.Activate properties [] {} >/dev/null & sleep 1.5; }
props a.t
[ "$BACKEND" = wayland ] || gt_check "Properties of a file: the usual order" "$(tabs)" "Basic Emblems Permissions Open With Checksums Notes "
to_front Notes Basic; gt_shot props-moved
gt_check "Notes moved to the front" "$(tabs)" "Notes Basic Emblems Permissions Open With Checksums "
gt_check "and kept (Media Info, not shown, still after Notes)" "$(grep '^props_tab_order=' "$XDG_CONFIG_HOME/arkha/arkha.conf")" "props_tab_order=notes;media;basic;emblems;permissions;open-with;checksums;"
gt_key Escape; sleep 0.5
props fol
gt_check "a folder's Properties: the same order, without a file's tabs" "$(tabs)" "Notes Basic Emblems Permissions "
gt_key Escape; sleep 0.5
props b.t
gt_check "another file: the order kept" "$(tabs)" "Notes Basic Emblems Permissions Open With Checksums "
gt_key Escape; sleep 0.5
# Rename All (two files selected).
gt_click 900 650; gt_type a.t; gt_key Escape; gt_key shift+Right
gdbus call --session --dest $APP_ID --object-path $APP_PATH/window/1 \
  --method org.gtk.Actions.Activate rename-all [] {} >/dev/null & sleep 1.5
if [ "$BACKEND" = wayland ]; then
  gt_check "Rename All: the order from the settings" "$(tabs)" "Regex Case Insert Remove Replace Enumerate "
else
  before=$(tabs)
  first=$(echo "$before" | awk '{print $1}'); last=$(gt_a11y dialog-tabs | tail -1)
  to_front "$last" "$first"; gt_shot rename-moved
  gt_check "Rename All: its last tab moved to the front" "$(tabs)" "$last $(echo "$before" | sed "s/ $last //; s/ $//") "
fi
gt_key Escape
gt_end
