#!/bin/bash
# ops-popover.sh x11|wayland: the operations popover, opened to follow a
# queue, stays open while the next queued job takes over. Two checked
# copies, the second queued; the popover is opened, the first copy ends:
# the popover still shows the second one (it closed as the list was empty
# for a moment, the button hiding).
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/first" "$HOME/second" "$HOME/to1" "$HOME/to2"
python3 -c "
import os
for d, n in (('first', 2000), ('second', 2500)):
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
popover() { local n; for n in "1 file operation" "2 file operations"; do gt_a11y press-in "*" "$n" && return; done; return 1; }
# What the popover shows of the second copy (its texts, when shown).
shown() { gt_a11y window-labels | grep -c '^Copying “second”'; }
copy first to1; sleep 1; popover; gt_a11y press-in "*" Pause; gt_key Escape
copy second to2
gt_check "the second copy waits" "$(find "$HOME/to2" -mindepth 1 | wc -l)" "0"
popover; sleep 0.5
gt_check "the popover shows both" "$(shown) $(gt_a11y window-labels | grep -c '^Copying “first”')" "1 1"
# Resumed from the popover itself, which stays open.
gt_a11y press-in "*" Resume
for _ in $(seq 1 2400); do [ -e "$HOME/to2/second" ] && break; sleep 0.05; done
sleep 1
gt_shot second-running
gt_check "the first copy is done, the second running" "$(ls "$HOME/to1/first" | wc -l) $([ -e "$HOME/to2/second" ] && echo started)" "2000 started"
gt_check "the popover is still open, showing the second" "$(shown)" "1"
for _ in $(seq 1 900); do [ "$(ls "$HOME/to2/second" 2>/dev/null | wc -l)" = 2500 ] && break; sleep 0.2; done
gt_end
