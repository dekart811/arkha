#!/bin/bash
# prefs.sh x11|wayland: every page of Preferences (a screenshot each), and
# adding, editing and removing a custom action, which is saved at once.
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

gt_display
gt_arkha "$HOME"
gt_prefs
gt_check "Preferences opens" "$(gt_a11y dialogs)" "File Manager Preferences"
# The page list's first row, then down the list.
gt_prefs_page 0
for i in $(seq 1 10); do gt_shot "page-$i"; gt_key Down; done
# Custom Actions: the ninth page.
gt_prefs_page 8
ax=437 ay=354; [ "$BACKEND" = wayland ] && ay=368
gt_click $ax $ay; gt_type Echo; gt_key Tab; gt_type 'echo %f'; gt_key Return; gt_shot action-added
gt_check "an action added is saved" "$(grep -c '^name=Echo' "$XDG_CONFIG_HOME/arkha/actions.conf" 2>/dev/null)" "1"
gt_click 500 $((ay - 116)); gt_key Return; gt_key Escape
gt_click 606 $ay; gt_shot action-removed
gt_check "and removed" "$(grep -c '^name=' "$XDG_CONFIG_HOME/arkha/actions.conf" 2>/dev/null)" "0"
gt_end
