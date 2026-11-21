#!/bin/bash
# ops-order.sh x11|wayland: the File Operations window lists the running
# job first and the waiting ones below it, in queue order, also once a
# queued job has started. Three checked copies: the first runs, the second
# and third wait. When the first ends and the second starts, the second
# has to be listed above the third (it was listed below it, so the third
# looked first in line).
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/first" "$HOME/second" "$HOME/third" "$HOME/to1" "$HOME/to2" "$HOME/to3"
python3 -c "
import os
for d, n in (('first', 4000), ('second', 2500), ('third', 50)):
    for i in range(n):
        with open(os.path.join('$HOME', d, 'f%04d.dat' % i), 'wb') as f:
            f.write(os.urandom(16384))
"
gt_display
gt_arkha "$HOME"
copy() {  # copy NAME TO: a checked copy of ~/NAME into ~/TO
  gt_go "$HOME" 0.3
  gt_click 900 650; gt_type "$1"; gt_key Escape; gt_key ctrl+c
  gt_go "$HOME/$2" 0; gt_key shift+ctrl+v; sleep 0.3
}
# The operations button's tooltip says how many operations there are.
popover() { local n; for n in "1 file operation" "2 file operations" "3 file operations"; do gt_a11y press-in "*" "$n" && return; done; return 1; }
# press TOOLTIP: presses that button in the operations popover (the first
# one, top to bottom), and closes the popover. Pressed through
# accessibility: Wayland tells nobody where a button is on the screen.
press() { popover; gt_a11y press-in "*" "$1"; sleep 0.3; gt_key Escape; }
# The copies here are quick: each running one is paused while it's looked at.
copy first to1; sleep 1; press Pause
copy second to2; copy third to3
gt_check "the second and third copies wait" "$(find "$HOME/to2" "$HOME/to3" -mindepth 1 | wc -l)" "0"
popover; gt_a11y press-in "*" "Open in Window"; sleep 1
gt_check "the File Operations window is open" "$(gt_a11y windows | grep -c '^File Operations$')" "1"
# The jobs' titles in the window, top to bottom.
order() { gt_a11y labels-in "File Operations" | grep -E '^Copying' | sed -E 's/^Copying “([a-z]+)”.*/\1/' | tr '\n' ' '; }
gt_check "while the first runs" "$(order)" "first second third "
# From now on the buttons in the File Operations window.
press() { gt_a11y press-in "File Operations" "$1"; sleep 0.3; }
press Resume
for _ in $(seq 1 2400); do [ -e "$HOME/to2/second" ] && break; sleep 0.05; done
gt_check "the first copy is done, the second started" "$(ls "$HOME/to1/first" | wc -l) $([ -e "$HOME/to2/second" ] && echo started)" "4000 started"
press Pause
gt_shot second-started
gt_check "once the second has started, it comes before the third" "$(order)" "second third "
press Resume
for _ in $(seq 1 900); do [ "$(ls "$HOME/to3/third" 2>/dev/null | wc -l)" = 50 ] && break; sleep 0.2; done
gt_check "all three copies done" "$(ls "$HOME/to1/first" | wc -l) $(ls "$HOME/to2/second" | wc -l) $(ls "$HOME/to3/third" | wc -l)" "4000 2500 50"
gt_end
