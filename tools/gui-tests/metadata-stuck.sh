#!/bin/bash
# metadata-stuck.sh x11|wayland: with GVfs's metadata service not
# answering (this session's gvfsd-metadata stopped with SIGSTOP, found by
# the session's D-Bus address, never the user's), Arkha must keep answering:
# reloading, opening another folder, pasting, a new folder and Properties
# each within 2 s (it used to wait up to 25 s). Its files are on another
# file system than the home folder (/dev/shm), so GVfs asks the service.
# GVfs's own warning that the service timed out is expected.
GT_WARNINGS=allow
. "$(dirname "$0")/lib.sh"
gt_session "$@"

T=/dev/shm/arkha-gt-$$; rm -rf "$T"; mkdir -p "$T/one/inner" "$T/two"
for n in a b c; do echo $n > "$T/one/$n.txt"; echo $n > "$T/two/$n.txt"; done
gt_display
gio set -t stringv "$T/one/a.txt" metadata::emblems important
META=""
for p in $(pgrep -x gvfsd-metadata); do
  tr '\0' '\n' < "/proc/$p/environ" 2>/dev/null | grep -qx "DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS" && META=$p
done
gt_ok "this session's metadata service found" test -n "$META"
gt_arkha "$T/one"
[ -n "$META" ] && kill -STOP "$META"
# The slowest answer to a D-Bus call while Arkha works for SECONDS.
slowest() {
  local end=$(( $(date +%s) + $1 )) worst=0 t0 ms
  while [ "$(date +%s)" -lt $end ]; do
    t0=$(date +%s%N)
    timeout 60 gdbus call --session --dest $APP_ID --object-path $APP_PATH --method org.gtk.Actions.List >/dev/null 2>&1
    ms=$(( ($(date +%s%N) - t0) / 1000000 )); [ $ms -gt $worst ] && worst=$ms; sleep 0.2
  done
  echo $worst
}
step() {  # step NAME SECONDS: PASS if Arkha answered within 2 s throughout
  local ms; ms=$(slowest "$2")
  if [ "$ms" -lt 2000 ]; then echo "PASS $1 (slowest answer $ms ms)" >> "$OUT/result"; else echo "FAIL $1: an answer took $ms ms" >> "$OUT/result"; fi
}
act() { timeout 60 gdbus call --session --dest $APP_ID --object-path $APP_PATH/window/1 \
  --method org.gtk.Actions.Activate "$1" "${2:-[]}" {} >/dev/null 2>&1 & }
act reload; step "reload" 4
act go-uri "[<'file://$T/two'>]"; step "another folder" 4
xdotool type --delay 60 a; sleep 0.5; xdotool key Escape; act copy; sleep 0.5
act go-uri "[<'file://$T/one/inner'>]"; sleep 1; act paste; step "paste" 6
act new-folder; sleep 1; xdotool key Return; step "new folder" 6
act properties; step "Properties" 6
gt_shot properties
xdotool key Escape
[ -n "$META" ] && kill -CONT "$META"
gt_end
rm -rf "$T"
