# Shared by the GUI tests and benchmarks (sourced, not run).
#
# A test calls gt_session first: outside a session it sets up a run folder
# (target/gui-tests/<test>-<backend>/) with a home and settings of its own,
# then runs the script again inside a private D-Bus session with its own
# runtime folder, so nothing of the user's desktop is touched (its GVfs, its
# Trash, its accessibility bus, its running Arkha). Inside, gt_display starts
# a headless X server, and xmonad (X11) or Weston's kiosk shell in it
# (Wayland). Afterwards the outer run prints the test's PASS and FAIL lines
# and the warnings in Arkha's log, and exits 1 if anything failed.
#
# The test's own variables:
#   GT_NAME      the test's name (default: the script's)
#   GT_CONF      lines for arkha.conf's [General] group
#   GT_A11Y=1    accessibility on (needed by gt_a11y)
#   GT_WARNINGS=allow   warnings in the log don't fail the test
#   GT_WM=none   no window manager on X11 (the benchmarks)
#   GT_WESTON_SHELL=desktop   Weston's desktop shell on Wayland (default: kiosk)
#   GT_WESTON_ARGS  more options for Weston (e.g. --xwayland)
#   GT_BENCH=1   a benchmark: its results are shown, nothing is checked
# From outside: ARKHA (the binary, default target/release/arkha).

GT_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
GT_DIR=$GT_ROOT/tools/gui-tests
GT_FIX=$GT_ROOT/target/gui-fixtures
ARKHA=${ARKHA:-$GT_ROOT/target/release/arkha}
APP_ID=io.github.dekart811.Arkha
APP_PATH=/io/github/dekart811/Arkha

# gt_session BACKEND [ARGS...]: see above. Returns only inside the session,
# with OUT (the run folder), BACKEND and HOME set.
gt_session() {
  BACKEND=${1:-x11}
  case "$BACKEND" in x11|wayland) ;; *) echo "usage: $0 x11|wayland" >&2; exit 2 ;; esac
  GT_NAME=${GT_NAME:-$(basename "$0" .sh)}
  local runs=gui-tests; [ -n "$GT_BENCH" ] && runs=bench-runs
  OUT=$GT_ROOT/target/$runs/$GT_NAME-$BACKEND
  if [ -n "$GT_INNER" ]; then
    export HOME=$OUT/home XDG_CONFIG_HOME=$OUT/config XDG_CACHE_HOME=$OUT/home/.cache XDG_DATA_HOME=$OUT/home/.local/share
    # English, whatever the system's language: the checks look for its words.
    export XDG_DATA_DIRS=$GT_FIX/share-noapps GSETTINGS_BACKEND=memory GVFS_DISABLE_FUSE=1 LANGUAGE=en
    if [ "$GT_A11Y" = 1 ]; then unset NO_AT_BRIDGE GTK_A11Y; else export NO_AT_BRIDGE=1 GTK_A11Y=none; fi
    cd "$OUT" || exit 1
    return 0
  fi
  [ -x "$ARKHA" ] || { echo "no $ARKHA: cargo build --release first" >&2; exit 2; }
  "$GT_DIR/fixtures.sh" base >/dev/null || exit 2
  rm -rf "$OUT"; mkdir -p "$OUT/home" "$OUT/config/arkha"
  printf '[General]\n%s\n' "${GT_CONF:-}" > "$OUT/config/arkha/arkha.conf"
  : > "$OUT/result"
  local rt status
  rt=$(mktemp -d /tmp/arkha-gt.XXXXXX)
  # The home is set before the session starts, so the services D-Bus starts
  # (GVfs's Trash and metadata) use the test home too, never the user's.
  HOME=$OUT/home XDG_CONFIG_HOME=$OUT/config XDG_CACHE_HOME=$OUT/home/.cache XDG_DATA_HOME=$OUT/home/.local/share \
    XDG_DATA_DIRS=$GT_FIX/share-noapps XDG_RUNTIME_DIR=$rt GT_INNER=1 \
    dbus-run-session -- bash "$0" "$@" > "$OUT/session.log" 2>&1
  status=$?
  sleep 0.5
  for d in "$rt/doc" "$rt/gvfs"; do [ -d "$d" ] && fusermount3 -uz "$d" 2>/dev/null; done
  rm -rf "$rt"
  [ $status -ne 0 ] && echo "FAIL the test script ended with status $status (see $OUT/session.log)" >> "$OUT/result"
  local warn
  warn=$(grep -hE "CRITICAL|WARNING|panicked" "$OUT"/arkha*.log 2>/dev/null | sed -E 's/\(arkha:[0-9]+\)/(arkha)/; s/[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]+/T/' | sort | uniq -c)
  if [ -n "$warn" ]; then
    if [ "$GT_WARNINGS" = allow ]; then echo "NOTE warnings in Arkha's log (allowed here):" >> "$OUT/result"
    else echo "FAIL warnings in Arkha's log:" >> "$OUT/result"; fi
    echo "$warn" | sed 's/^/       /' >> "$OUT/result"
  fi
  echo "== $GT_NAME on $BACKEND"
  cat "$OUT/result"
  [ -n "$GT_BENCH" ] || grep -q '^PASS\|^FAIL' "$OUT/result" || echo "FAIL nothing was checked (see $OUT/session.log)"
  echo "   screenshots and logs: $OUT"
  ! grep -q '^FAIL' "$OUT/result"
  exit $?
}

# gt_display [WIDTH HEIGHT]: the X server, and the window manager for BACKEND.
gt_display() {
  local w=${1:-1200} h=${2:-800} r num
  r=$(mktemp -u); mkfifo "$r"
  Xvfb -displayfd 9 -screen 0 "${w}x${h}x24" -nolisten tcp 9>"$r" > "$OUT/xvfb.log" 2>&1 &
  GT_PIDS="$!"
  read -r num < "$r"; rm -f "$r"
  export DISPLAY=:$num
  if [ "$GT_A11Y" = 1 ]; then
    /usr/lib/at-spi-bus-launcher --launch-immediately >/dev/null 2>&1 & GT_PIDS="$GT_PIDS $!"; sleep 0.5
    /usr/lib/at-spi2-registryd >/dev/null 2>&1 & GT_PIDS="$GT_PIDS $!"; sleep 0.5
  fi
  if [ "$BACKEND" = wayland ]; then
    weston --backend=x11 --shell=${GT_WESTON_SHELL:-kiosk} ${GT_WESTON_ARGS:-} --width="$w" --height="$h" --socket=wayland-gt --idle-time=0 > "$OUT/weston.log" 2>&1 &
    GT_PIDS="$GT_PIDS $!"
    for _ in $(seq 50); do [ -S "$XDG_RUNTIME_DIR/wayland-gt" ] && break; sleep 0.1; done; sleep 0.5
    export WAYLAND_DISPLAY=wayland-gt GDK_BACKEND=wayland
  elif [ "$GT_WM" = none ]; then
    export GDK_BACKEND=x11
  else
    XMONAD_CONFIG_DIR=$GT_FIX/xmonad XMONAD_CACHE_DIR=$GT_FIX/xmonad XMONAD_DATA_DIR=$GT_FIX/xmonad xmonad > "$OUT/xmonad.log" 2>&1 &
    GT_PIDS="$GT_PIDS $!"; sleep 1
    export GDK_BACKEND=x11
  fi
}

# gt_arkha ARGS...: starts Arkha (log in arkha.log, appended), waits until its
# first window is there and the main thread is quiet. ARKHA_PID is set.
gt_arkha() {
  "$ARKHA" "$@" >> "$OUT/arkha.log" 2>&1 &
  ARKHA_PID=$!
  for _ in $(seq 100); do
    gdbus introspect --session --dest $APP_ID --object-path $APP_PATH/window/1 >/dev/null 2>&1 && break
    sleep 0.1
  done
  gt_quiet
  # Weston's window gets the keys and pointer only once focused.
  [ "$BACKEND" = wayland ] && xdotool search --classname weston windowfocus --sync 2>/dev/null
  [ -n "$GT_BENCH" ] || { xdotool mousemove 600 450; sleep 0.2; }
}

# gt_quiet: waits until Arkha's main thread has used no CPU for 0.4 s (at most 20 s).
gt_quiet() {
  local last t quiet=0 n=0
  last=$(awk '{print $14+$15}' /proc/$ARKHA_PID/task/$ARKHA_PID/stat 2>/dev/null)
  while [ $quiet -lt 4 ] && [ $n -lt 200 ]; do
    sleep 0.1; n=$((n+1))
    t=$(awk '{print $14+$15}' /proc/$ARKHA_PID/task/$ARKHA_PID/stat 2>/dev/null) || return
    if [ "$t" = "$last" ]; then quiet=$((quiet+1)); else quiet=0; fi; last=$t
  done
}

# gt_act ACTION [PARAM [WINDOW]]: a window action over D-Bus, as menus call
# them (PARAM in GVariant text, e.g. "<'list'>"), then waits for quiet.
gt_act() {
  local p=${2:+[$2]}
  timeout 60 gdbus call --session --dest $APP_ID --object-path "$APP_PATH/window/${3:-1}" \
    --method org.gtk.Actions.Activate "$1" "${p:-[]}" {} >/dev/null
  gt_quiet
}

# Keyboard and mouse (real XTEST input into the X server, or Weston's window).
gt_key() { xdotool key --delay 80 "$@"; sleep 0.4; gt_quiet; }
gt_type() { xdotool type --delay 50 "$1"; sleep 0.3; }
gt_click() { xdotool mousemove "$1" "$2" click "${3:-1}"; sleep 0.5; gt_quiet; }

# gt_shot NAME: a screenshot of the whole screen, NAME.png in the run folder.
gt_shot() { import -window root "$OUT/$1.png" 2>/dev/null; }

# gt_a11y QUERY: what accessibility shows of Arkha (GT_A11Y=1): see a11y.py.
# Each query is an accessibility client that comes and goes. GTK 3 crashes
# when a key arrives while its AT-SPI bridge takes its key snooper away for
# a client that left (it calls the freed snooper; seen as segfaults in
# gtk_main_do_event): so a moment's pause after it, before any key.
gt_a11y() { python3 "$GT_DIR/a11y.py" "$@"; local s=$?; sleep 0.3; return $s; }

# gt_note TEXT: a result line (benchmarks).
gt_note() { echo "    $*" >> "$OUT/result"; }

# gt_check NAME GOT WANT: PASS if they're equal, else FAIL with both.
gt_check() {
  if [ "$2" = "$3" ]; then echo "PASS $1" >> "$OUT/result"
  else printf 'FAIL %s\n       got:  %s\n       want: %s\n' "$1" "$2" "$3" >> "$OUT/result"; fi
}

# gt_ok NAME COMMAND...: PASS if the command succeeds.
gt_ok() {
  local name=$1; shift
  if "$@"; then echo "PASS $name" >> "$OUT/result"; else echo "FAIL $name" >> "$OUT/result"; fi
}

# gt_quit: quits Arkha (as File → Quit does), killed if it doesn't end.
gt_quit() {
  if [ -n "$ARKHA_PID" ] && kill -0 "$ARKHA_PID" 2>/dev/null; then
    timeout 10 gdbus call --session --dest $APP_ID --object-path $APP_PATH \
      --method org.gtk.Actions.Activate quit [] {} >/dev/null 2>&1
    for _ in $(seq 30); do kill -0 "$ARKHA_PID" 2>/dev/null || break; sleep 0.1; done
    kill "$ARKHA_PID" 2>/dev/null && echo "NOTE Arkha didn't quit within 3 s" >> "$OUT/result"
    wait "$ARKHA_PID" 2>/dev/null
  fi
  ARKHA_PID=
}

# gt_end: quits Arkha and stops the display.
gt_end() {
  gt_quit
  # shellcheck disable=SC2086
  kill $GT_PIDS 2>/dev/null; wait 2>/dev/null
}
