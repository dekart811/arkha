#!/bin/bash
# xds-drag.sh x11|wayland: an item dragged out of an archive manager that
# uses X Direct Save (Engrampa, Xarchiver, older File Roller) into the file
# view is saved there by the archive manager. XDS only works between X11
# programs: on X11, and on Wayland when both run through XWayland
# (GDK_BACKEND=x11), which this checks. Here XWayland runs as one window
# filling Weston's screen ("rootful"), with a pointer of its own that
# xdotool can move: the usual XWayland, each of its windows one of
# Weston's, takes simulated pointer input from neither the outer X server
# (through the nested Weston) nor XTEST. The XDS exchange between two X11
# programs is the X server's either way. A stand-in archive manager
# (fake-archiver.py) writes from-xds.txt where it's told; its magenta
# window is found in a screenshot.
GT_WESTON_SHELL=desktop GT_CONF='default_view=list
width=1200
height=760'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

D=$HOME/dest; mkdir -p "$D"
LOG=$OUT/fake.log
gt_display 1200 800
OUTER=$DISPLAY
if [ "$BACKEND" = wayland ]; then
  # Both through XWayland, as one window filling Weston's screen.
  r=$(mktemp -u); mkfifo "$r"
  Xwayland -displayfd 9 -geometry 1200x800 -fullscreen -noreset 9>"$r" > "$OUT/xwayland.log" 2>&1 & GT_PIDS="$GT_PIDS $!"
  read -r num < "$r"; rm -f "$r"; XW=:$num
  sleep 1.5
  gt_check "XWayland runs in Weston" "$(DISPLAY=$XW xdotool getdisplaygeometry 2>/dev/null)" "1200 800"
  export DISPLAY=$XW GDK_BACKEND=x11
fi
# Arkha first, the stand-in over it (and focused).
"$ARKHA" "$D" >> "$OUT/arkha.log" 2>&1 & ARKHA_PID=$!; GT_PIDS="$GT_PIDS $ARKHA_PID"
sleep 3
python3 "$GT_DIR/fake-archiver.py" xds "$LOG" > "$OUT/fake.out" 2>&1 & GT_PIDS="$GT_PIDS $!"
for _ in $(seq 50); do grep -q '^ready' "$LOG" 2>/dev/null && break; sleep 0.1; done; sleep 1.5
# The pointer and screenshots are the outer X server's (Weston's window).
export DISPLAY=$OUTER
gt_shot before
read -r fw fh fx fy < <(magick "$OUT/before.png" -fill black +opaque '#ff00ff' -fill white -opaque '#ff00ff' -trim -format '%w %h %X %Y' info: | tr -d '+')
gt_check "the stand-in's window is on the screen" "$([ "${fw:-0}" -gt 50 ] && echo yes)" "yes"
sx=$((fx + fw / 2)) sy=$((fy + fh / 2))
if [ "$sx" -gt 600 ]; then tx=300; else tx=900; fi; ty=500
# The drag: with the X server's pointer on X11, with XWayland's own on
# Wayland (its window fills Weston's screen: the same places on both).
[ "$BACKEND" = wayland ] && export DISPLAY=$XW
xdotool mousemove $sx $sy; sleep 0.3; xdotool mousedown 1; sleep 0.3
for i in $(seq 1 15); do xdotool mousemove $((sx + i * (tx - sx) / 15)) $((sy + i * (ty - sy) / 15)); sleep 0.05; done
sleep 0.3; xdotool mouseup 1
export DISPLAY=$OUTER
for _ in $(seq 30); do [ -e "$D/from-xds.txt" ] && break; sleep 0.2; done
gt_shot after
gt_check "the archive manager was told to save into the folder" "$(grep -c "^save file://$D/from-xds.txt\$" "$LOG")" "1"
gt_check "and the file is there" "$(cat "$D/from-xds.txt" 2>/dev/null)" "from XDS"
gt_end
