#!/bin/bash
# focuscol.sh x11|wayland: with accessibility on and a client listening (as
# a screen reader does), a click on a file's Size cell in the list view,
# then the columns view (which hides that column) and cursor moves. GTK
# kept the hidden column as the cursor's, and its accessibility then failed
# an assertion (get_column_number) at every cursor move: the log must stay
# clean.
GT_A11Y=1 GT_CONF='default_view=list
per_folder=false'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/files/sub"; for n in a b c d e; do echo "$n$n$n" > "$HOME/files/$n.txt"; done
gt_display
gt_arkha "$HOME/files"
gt_a11y windows >/dev/null     # makes the accessible tree
gt_a11y listen 40 & L=$!
sleep 1
# The second row's Size cell (a.txt).
y=172
gt_click 870 $y; gt_shot clicked
gt_act view-mode "<'columns'>"; gt_shot columns
gt_key Down Down; gt_act reload; gt_act view-mode "<'list'>"; gt_act view-mode "<'columns'>"; gt_act reload
gt_shot moved
gt_act view-mode "<'list'>"; gt_shot list
gt_check "the list view's columns are back" "$(gt_title)" "files"
kill $L 2>/dev/null
gt_end
