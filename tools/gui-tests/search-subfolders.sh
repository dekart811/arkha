#!/bin/bash
# search-subfolders.sh x11|wayland: "include subfolders" in the search's
# Location row. Ticked (the default), a search looks in the folders inside
# the one searched too; unticked (searched again at once, no Reload), only
# in the folder itself: a folder whose name matches is found, what's in it
# isn't.
# With Preferences → Behavior → "Include subfolders" off, a new
# search starts unticked; the Preferences box writes that setting.
GT_A11Y=1 GT_CONF='default_view=list'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

D=$HOME/Documents; mkdir -p "$D/invoices/old"
touch "$D/invoice-1.pdf" "$D/notes.txt" "$D/invoices/invoice-2.pdf" "$D/invoices/old/invoice-3.pdf"
# The results' names, sorted.
results() { gt_a11y cells | grep -E '^invoice' | sort | tr '\n' ' '; }
ALL="invoice-1.pdf invoice-2.pdf invoice-3.pdf invoices "
HERE="invoice-1.pdf invoices "
gt_display
gt_arkha "$D"
gt_key ctrl+f; gt_type invoice; gt_key Return; sleep 2
gt_check "ticked (the default): the subfolders too" "$(results)" "$ALL"
gt_ok "untick include subfolders" gt_a11y press-in "*" "include subfolders"
sleep 2
gt_shot only-here
gt_check "unticked: only the folder itself" "$(results)" "$HERE"
gt_quit

# Off in Preferences: a new search starts unticked; ticked, all again.
printf '[General]\ndefault_view=list\nsearch_subfolders=false\n' > "$XDG_CONFIG_HOME/arkha/arkha.conf"
gt_arkha "$D"
gt_key ctrl+f; gt_type invoice; gt_key Return; sleep 2
gt_check "setting off: a new search only in the folder" "$(results)" "$HERE"
gt_ok "tick include subfolders" gt_a11y press-in "*" "include subfolders"
sleep 2
gt_check "ticked again: the subfolders too" "$(results)" "$ALL"

# The Preferences box: ticked (from the file), pressed, saved as on.
# (Started apart: the call returns only once the window closes.)
gdbus call --session --dest $APP_ID --object-path $APP_PATH/window/1 \
  --method org.gtk.Actions.Activate preferences [] {} >/dev/null & sleep 2
# Its second page, Behavior (as prefs.sh finds the pages).
y=180; [ "$BACKEND" = wayland ] && y=194
gt_click 240 $y; gt_key Down; sleep 0.5
# Scrolled down to the Search section (shown, accessibility has it), over
# the labels on the left: a choice under the pointer would change.
xdotool mousemove 380 450; for _ in $(seq 10); do xdotool click 5; done; sleep 0.5
gt_shot prefs
gt_ok "press Include subfolders" gt_a11y press "Include subfolders"; sleep 1
gt_check "saved" "$(grep -c '^search_subfolders=true' "$XDG_CONFIG_HOME/arkha/arkha.conf")" "1"
gt_end
