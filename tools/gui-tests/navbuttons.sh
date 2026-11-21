#!/bin/bash
# navbuttons.sh x11|wayland: the mouse's back and forward buttons over the
# views, as set in Preferences (buttons 8 and 9, swapped, or turned off), in
# the icon, list and compact views; then a right click on an item and on
# empty space, and a middle click on a folder (it opens in a new tab).
# Arkha starts in nav/a/b and goes up twice (to nav), then a button is
# clicked over empty space in the view: it must show "a" (back), and after
# the other button "nav" again.
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/nav/a/b"; touch "$HOME/nav/file1" "$HOME/nav/a/file2"
gt_display
n=0
while read -r name view settings b1 b2 want1 want2; do
  n=$((n+1))
  printf '[General]\ndefault_view=%s\n%s\n' "$view" "$(echo "$settings" | tr ',' '\n')" > "$XDG_CONFIG_HOME/arkha/arkha.conf"
  gt_arkha "$HOME/nav/a/b"
  gt_act up; gt_act up
  xdotool mousemove 900 650; sleep 0.2
  gt_click 900 650 "$b1"; gt_shot "$name-1"; gt_check "$name: button $b1" "$(gt_title)" "$want1"
  gt_click 900 650 "$b2"; gt_shot "$name-2"; gt_check "$name: button $b2" "$(gt_title)" "$want2"
  if [ $n -le 2 ]; then
    # Right click on empty space: the folder's menu, nothing selected.
    gt_click 900 650 3; gt_shot "$name-menu-space"; gt_key Escape
    # On the first item: it's selected, and its menu.
    gt_click 280 150 3; gt_shot "$name-menu-item"; gt_key Escape
    # Middle click on it (the folder "a"): opened in a new tab.
    gt_click 280 150 2; gt_shot "$name-middle"
    gt_check "$name: middle click opens a tab" "$(gt_a11y tabs | tr '\n' ' ')" "nav a "
  fi
  gt_quit
done <<'CASES'
icons icon none=0 8 9 a nav
list list none=0 8 9 a nav
compact compact none=0 8 9 a nav
icons-swapped icon mouse_back_button=9,mouse_forward_button=8 9 8 a nav
list-swapped list mouse_back_button=9,mouse_forward_button=8 9 8 a nav
list-off list mouse_extra_buttons=false 8 9 nav nav
CASES
gt_end
