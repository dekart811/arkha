#!/bin/bash
# archive-drag.sh x11|wayland: an item dragged out of KDE's Ark into the
# file view is extracted there: Arkha asks Ark (over D-Bus, with the
# connection name and object path the drag carries) to extract into the
# folder, as Dolphin does, on X11 and Wayland alike. A stand-in Ark
# (fake-archiver.py) writes from-ark.txt where it's told. On Wayland,
# Weston's desktop shell shows both windows; Arkha fills the screen, the
# stand-in's magenta window is found in a screenshot.
GT_WESTON_SHELL=desktop GT_CONF='default_view=list
width=1200
height=760'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

D=$HOME/dest; mkdir -p "$D"
LOG=$OUT/fake.log
gt_display 1200 800
gt_arkha "$D"
python3 "$GT_DIR/fake-archiver.py" ark "$LOG" > "$OUT/fake.out" 2>&1 & GT_PIDS="$GT_PIDS $!"
for _ in $(seq 50); do grep -q '^ready' "$LOG" 2>/dev/null && break; sleep 0.1; done; sleep 1.5
gt_shot before
# The stand-in: the middle of the magenta in the screenshot.
read -r fw fh fx fy < <(magick "$OUT/before.png" -fill black +opaque '#ff00ff' -fill white -opaque '#ff00ff' -trim -format '%w %h %X %Y' info: | tr -d '+')
gt_check "the stand-in's window is on the screen" "$([ "${fw:-0}" -gt 50 ] && echo yes)" "yes"
sx=$((fx + fw / 2)) sy=$((fy + fh / 2))
# Onto the file list, on the other half of the screen.
if [ "$sx" -gt 600 ]; then tx=300; else tx=900; fi; ty=500
xdotool mousemove $sx $sy; sleep 0.3; xdotool mousedown 1; sleep 0.3
for i in $(seq 1 15); do xdotool mousemove $((sx + i * (tx - sx) / 15)) $((sy + i * (ty - sy) / 15)); sleep 0.05; done
sleep 0.3; xdotool mouseup 1
for _ in $(seq 30); do [ -e "$D/from-ark.txt" ] && break; sleep 0.2; done
gt_shot after
gt_check "Ark was asked to extract into the folder" "$(grep -c "^extract $D\$" "$LOG")" "1"
gt_check "and the file is there" "$(cat "$D/from-ark.txt" 2>/dev/null)" "from Ark"
gt_end
