#!/bin/bash
# ops-queue.sh x11|wayland: the File Operations window stays open while a
# queued job takes over. A checked copy of 4,000 files runs (Paste and
# Check: each copy read back, so it takes a while), another checked copy
# (1,500 files, so it takes a while too) is queued behind it, and Arkha is told to quit: with jobs left it shows the
# File Operations window instead (as its button does) and closes the main
# window. The window has to stay until the second copy is done too (it
# closed as the queued job started), then close, and Arkha end.
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/big" "$HOME/second" "$HOME/to1" "$HOME/to2"
python3 -c "
import os
for d, n in (('big', 4000), ('second', 1500)):
    for i in range(n):
        with open(os.path.join('$HOME', d, 'f%04d.dat' % i), 'wb') as f:
            f.write(os.urandom(16384))
"
# The second copy's files so far.
second() { ls "$HOME/to2/second" 2>/dev/null | wc -l; }
gt_display
gt_arkha "$HOME"
# Is the File Operations window up? Seen with xdotool on X11: with the
# main window gone, accessibility lists none of Arkha's windows, and on
# Wayland nothing outside sees them, so there only the copies and the end
# are checked.
ops_up() { xdotool search --onlyvisible --name "^File Operations$" >/dev/null 2>&1; }
# The operations button's tooltip says how many there are; pressed, then
# a button in its popover (through accessibility, as on Wayland nothing
# outside knows where it is), and the popover closed.
press() {
  local n; for n in "1 file operation" "2 file operations"; do gt_a11y press-in "*" "$n" && break; done
  gt_a11y press-in "*" "$1"; sleep 0.3; gt_key Escape
}

gt_click 900 650; gt_type big; gt_key Escape; gt_key ctrl+c
gt_go "$HOME/to1" 0; gt_key shift+ctrl+v; sleep 1
# Paused until the second is queued: done before that, the second would
# just start (on a fast machine it sometimes was).
press Pause
gt_go "$HOME" 0.3
gt_click 900 650; gt_type second; gt_key Escape; gt_key ctrl+c
gt_go "$HOME/to2" 0; gt_key shift+ctrl+v; sleep 0.3
gt_check "the second copy is queued (the first still runs)" "$([ -e "$HOME/to2/second" ] && echo started || echo queued)" "queued"
press Resume
timeout 10 gdbus call --session --dest $APP_ID --object-path $APP_PATH \
  --method org.gtk.Actions.Activate quit [] {} >/dev/null
sleep 1
if [ "$BACKEND" = x11 ]; then
  gt_check "quitting with jobs left shows File Operations" "$(ops_up && echo yes || echo no)" "yes"
  # Until the second copy is done: the window may not close before.
  closed_early=no
  for _ in $(seq 1 600); do
    [ "$(second)" = 1500 ] && break
    ops_up || { closed_early="yes, with $(second) of the second copy's 1500 files there"; break; }
    sleep 0.1
  done
  gt_shot after-queue
  gt_check "the window stays while the queued copy takes over" "$closed_early" "no"
fi
for _ in $(seq 1 600); do [ "$(second)" = 1500 ] && break; sleep 0.1; done
gt_check "both copies done" "$(ls "$HOME/to1/big" | wc -l) $(second)" "4000 1500"
# Then it closes, and Arkha ends.
for _ in $(seq 1 50); do kill -0 "$ARKHA_PID" 2>/dev/null || break; sleep 0.2; done
gt_check "after the last job Arkha ends" "$(kill -0 "$ARKHA_PID" 2>/dev/null && echo running || echo ended)" "ended"
gt_end
