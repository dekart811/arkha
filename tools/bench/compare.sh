#!/bin/bash
# compare.sh [PROGRAMS...]: the audit's numbers, Arkha against Caja and Nemo
# (those installed), each in the same headless X server (no window manager)
# and private session, with no applications to open files with:
#   opening /usr/lib and 20,000 files, 3 times each (medians are what the
#     audit shows): when the main thread stays idle, its CPU, all threads';
#   CPU for 150 mouse-wheel steps down the open folder;
#   memory and threads with the folder open;
#   thumbnails of 30 or so file types with none cached (how many, CPU of
#     the program and its thumbnailers, when the last one appeared), and of
#     /usr/share/backgrounds/gnome if it's there.
# Close other busy programs first. Takes about 10 minutes.
GT_BENCH=1 GT_WM=none
D=$(cd "$(dirname "$0")" && pwd)
"$D/../gui-tests/fixtures.sh" bench >/dev/null || exit 2
. "$D/../gui-tests/lib.sh"
GT_NAME=compare
[ -z "$GT_INNER" ] && export PROGS="${*:-arkha caja nemo}"
gt_session x11
progs=$PROGS

gt_display 1280 800
start() {  # start PROGRAM DIR: P is its pid
  case "$1" in
    arkha) "$ARKHA" "$2" >> "$OUT/arkha.log" 2>&1 & ;;
    caja) caja --no-desktop "$2" >/dev/null 2>&1 & ;;
    *) "$1" "$2" >/dev/null 2>&1 & ;;
  esac
  P=$!
}
stop() { kill "$P" 2>/dev/null; wait "$P" 2>/dev/null; sleep 1; }
installed() { [ "$1" = arkha ] || command -v "$1" >/dev/null; }
reset() { rm -f "$XDG_CONFIG_HOME/arkha/folders.conf"; rm -rf "$HOME/.cache/thumbnails"; }

for dir in /usr/lib "$GT_FIX/big20k"; do
  gt_note "##### $dir"
  for p in $progs; do
    installed "$p" || continue
    for r in 1 2 3; do
      reset; t0=$(date +%s%N); start "$p" "$dir"
      last=0 quiet=0 busy=0
      while true; do
        sleep 0.1
        t=$(awk '{print $14+$15}' /proc/$P/task/$P/stat 2>/dev/null) || break
        all=$(awk '{print $14+$15}' /proc/$P/stat 2>/dev/null)
        now=$(( ($(date +%s%N) - t0) / 1000000 ))
        if [ "$t" = "$last" ]; then quiet=$((quiet+1)); else quiet=0; busy=$now; fi; last=$t
        { [ $quiet -ge 6 ] && [ $now -gt 1500 ]; } || [ $now -gt 30000 ] && break
      done
      gt_note "$p run $r: main thread busy until $busy ms, cpu $((t*10)) ms, all threads $((all*10)) ms"
      stop
    done
  done
  for p in $progs; do
    installed "$p" || continue
    reset; start "$p" "$dir"; sleep 6
    w=$(xdotool search --onlyvisible --pid $P 2>/dev/null | head -1)
    [ -z "$w" ] && w=$(xdotool search --onlyvisible --class "$p" | tail -1)
    xdotool windowsize "$w" 1100 700; sleep 1; xdotool mousemove --window "$w" 600 400; sleep 0.5
    a=$(awk '{print $14+$15}' /proc/$P/stat)
    for _ in $(seq 150); do xdotool click 5; sleep 0.02; done; sleep 1
    b=$(awk '{print $14+$15}' /proc/$P/stat)
    gt_note "$p: $(( (b-a)*10 )) ms CPU for 150 wheel steps"
    gt_note "$p: $(awk '/VmRSS/{print int($2/1024)}' /proc/$P/status) MB RSS, $(ls /proc/$P/task | wc -l) threads"
    stop
  done
done

for dir in "$GT_FIX/thumbtest" /usr/share/backgrounds/gnome; do
  [ -d "$dir" ] || continue
  gt_note "##### thumbnails $dir"
  for p in $progs; do
    installed "$p" || continue
    [ "$p" = nemo ] && continue   # not measured in the audits so far
    reset; t0=$(date +%s%N); start "$p" "$dir"
    last=-1 same=0
    while true; do
      sleep 0.5
      n=$(find "$HOME/.cache/thumbnails" -name '*.png' -not -path '*/fail/*' 2>/dev/null | wc -l)
      if [ "$n" = "$last" ]; then same=$((same+1)); else same=0; fi; last=$n
      [ $same -ge 6 ] && break; [ $(( ($(date +%s%N) - t0) / 1000000000 )) -gt 60 ] && break
    done
    read -r u s cu cs < <(awk '{print $14, $15, $16, $17}' /proc/$P/stat)
    kids=0; for c in $(pgrep -P "$P"); do k=$(awk '{print $14+$15+$16+$17}' "/proc/$c/stat" 2>/dev/null); kids=$((kids+${k:-0})); done
    gt_note "$p: $n thumbnails, own $(( (u+s)*10 )) ms, children $(( (cu+cs+kids)*10 )) ms, done after $(( ($(date +%s%N) - t0) / 1000000 - 3000 )) ms"
    stop
  done
done
gt_end
