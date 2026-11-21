#!/bin/bash
# ops-window.sh x11|wayland: with file operations in their own window (a
# setting), a copy that takes a while opens the File Operations window by
# itself, the location bar has no operations button, and the window closes
# once the copy is done.
GT_A11Y=1 GT_CONF='ops_window=true'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/big" "$HOME/to"
python3 -c "
import os
for i in range(4000):
    with open(os.path.join('$HOME', 'big', 'f%04d.dat' % i), 'wb') as f:
        f.write(os.urandom(16384))
"
gt_display
gt_arkha "$HOME"
win() { gt_a11y windows | grep -c '^File Operations$'; }
button() { gt_a11y where "1 file operation" >/dev/null && echo shown || echo none; }
gt_click 900 650; gt_type big; gt_key Escape; gt_key ctrl+c
gt_go "$HOME/to" 0; gt_key shift+ctrl+v; sleep 1.5
# Held while it's looked at.
gt_a11y press-in "File Operations" Pause
gt_shot running
gt_check "the File Operations window opened by itself" "$(win)" "1"
gt_check "no operations button in the location bar" "$(button)" "none"
gt_a11y press-in "File Operations" Resume
for _ in $(seq 1 900); do [ "$(ls "$HOME/to/big" 2>/dev/null | wc -l)" = 4000 ] && break; sleep 0.2; done
sleep 1.5
gt_check "the copy is done, the window closed" "$(ls "$HOME/to/big" | wc -l) $(win)" "4000 0"
gt_end
