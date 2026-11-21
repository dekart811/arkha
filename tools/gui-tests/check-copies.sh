#!/bin/bash
# check-copies.sh x11|wayland: copies checked against their originals,
# started the three ways there are: Edit → Paste and Check (Shift+Ctrl+V),
# the Copy To dialog's "Check the copies" box, and "Copy and Check Here" in
# the menu of a drop made with Alt. Each copy must be there and Arkha's log
# must show it was read back and matched; plain copies aren't checked.
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

W=$HOME/work; mkdir -p "$W/dest" "$W/to" "$W/plain"
head -c 3000000 /dev/urandom > "$W/a.bin"; echo hello > "$W/b.txt"
export G_MESSAGES_DEBUG=arkha
checks() { grep -c "check: .* matches" "$OUT/arkha.log"; }
# checked N: waits (at most 15 s) until N copies have been checked: a copy
# is read back and compared on a worker thread after it's made, which can
# take a while on a busy disk, so the main thread being idle says nothing.
checked() { local n=150; while [ $n -gt 0 ] && [ "$(checks)" -lt "$1" ]; do sleep 0.1; n=$((n - 1)); done; }
gt_display
gt_arkha "$W"
# A plain paste: no checking.
gt_click 900 650; gt_type b.t; gt_key Escape; gt_key ctrl+c
gt_go "$W/plain" 0; gt_key ctrl+v; sleep 1; gt_quiet
gt_check "a plain paste copies without checking" "$(ls "$W/plain") $(checks)" "b.txt 0"
# Paste and Check.
gt_key BackSpace; sleep 0.5; gt_quiet
gt_click 900 650; gt_type a.b; gt_key Escape; gt_key ctrl+c
gt_go "$W/dest" 0; gt_key shift+ctrl+v; checked 1; gt_quiet
gt_check "Paste and Check: copied and checked" "$(cmp -s "$W/a.bin" "$W/dest/a.bin" && echo same) $(checks)" "same 1"
gt_key BackSpace; sleep 0.5; gt_quiet
# Copy To with its box ticked: the dialog starts in the folder shown, so
# the folder "to" is typed in.
gt_click 900 650; gt_type b.t; gt_key Escape
gdbus call --session --dest $APP_ID --object-path $APP_PATH/window/1 \
  --method org.gtk.Actions.Activate copy-to [] {} >/dev/null & sleep 1.5
gt_shot copy-to
gt_ok "the Copy To dialog's box" gt_a11y press "Check the copies"
gt_key ctrl+l; gt_type "$W/to/"; sleep 0.5; gt_key Return; checked 2; gt_quiet
gt_check "Copy To with the box ticked: copied and checked" "$(ls "$W/to") $(checks)" "b.txt 2"
# A drop with Alt held asks what to do: "Copy and Check Here" (its key E).
# The view: dest, plain, to, a.bin, b.txt in the first row.
gt_shot before-drop
xdotool keydown alt; xdotool mousemove 775 150 mousedown 1; sleep 0.3
for i in $(seq 1 10); do xdotool mousemove $((775 - i * 165 / 10)) 150; sleep 0.05; done
xdotool mouseup 1; xdotool keyup alt; sleep 0.8; gt_shot drop-menu
gt_key e; checked 3; gt_quiet
gt_check "Copy and Check Here: copied and checked" "$(ls "$W/to" | tr '\n' ' ')$(checks)" "a.bin b.txt 3"
gt_check "and the original left where it was" "$(ls "$W" | tr '\n' ' ')" "a.bin b.txt dest plain to "
gt_end
