#!/bin/bash
# Checks View → Lock Toolbar with real mouse input: a bar button is dragged
# onto another group with the bar unlocked, locked, locked from the menu
# while running, and unlocked again.
#
#   tools/lockbar-test.sh [x11|wayland|both]     (default: both)
#
# Everything runs on a private display (Xvfb :71; for Wayland, Weston's
# kiosk shell nested in it), in its own D-Bus session and runtime folder
# (services started in the session, such as the accessibility bus, would
# otherwise put their sockets over the desktop's in /run/user) and with its own settings, so the real
# desktop, settings and a running Arkha are never touched. X11 runs under
# xmonad (as on the author's system), if it's installed.
#
# A drag counts as started when Arkha logs "bar drag started" (a GLib debug
# message, shown with G_MESSAGES_DEBUG=arkha), and as dropped when the bar
# order in the settings changes: the view buttons, dropped on the right half
# of the zoom group, go behind it. Every run starts with the order written
# out, so saving the settings for another reason doesn't look like a drop.
#
# Both fill the screen with the window (xmonad and the kiosk shell), so
# the same coordinates fit both: the default theme (Adwaita) on a 1200×800
# screen. Set DRAG ("x1 y1 x2 y2") for another one. View → Lock Toolbar is
# chosen by clicking View and pressing Up (its last item) and Return; MENU
# ("View x y Lock-Toolbar x y") clicks it instead.
#
# Needs: Xvfb, xdotool, dbus-run-session, weston (for Wayland), a release build.

set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=$ROOT/target/release/arkha
OUT=$ROOT/target/lockbar-test
DRAG=${DRAG:-"908 87 1128 87"}
MENU=${MENU:-}

if [ -z "${LOCKBAR_INNER:-}" ]; then
    [ -x "$BIN" ] || { echo "Build first: cargo build --release"; exit 2; }
    mkdir -p "$OUT"
    # A short path: Wayland sockets allow 107 bytes.
    XDG_RUNTIME_DIR=$(mktemp -d /tmp/arkha-rt.XXXXXX)
    export XDG_RUNTIME_DIR
    trap 'rm -rf "$XDG_RUNTIME_DIR"' EXIT
    fails=0
    for b in ${1:-both}; do
        case $b in
            both) set -- x11 wayland ;;
            *) set -- "$b" ;;
        esac
        for backend in "$@"; do
            # (gnome-keyring, started for the portal, prints a line of its own.)
            LOCKBAR_INNER=1 dbus-run-session -- "$0" "$backend" 2> "$OUT/$backend.bus.log" | grep --line-buffered -v '^GNOME_KEYRING'
            [ "${PIPESTATUS[0]}" = 0 ] || fails=$((fails + 1))
        done
    done
    [ $fails = 0 ] && echo "All passed." || echo "$fails backend(s) failed; logs and screenshots in $OUT"
    exit $((fails > 0))
fi

# ---- inside the private D-Bus session: one backend
backend=$1
fails=0
CFG=$OUT/$backend-config
LOG=$OUT/$backend.log
export DISPLAY=:71 GSETTINGS_BACKEND=memory GVFS_DISABLE_FUSE=1 NO_AT_BRIDGE=1 GTK_A11Y=none XDG_CONFIG_HOME=$CFG G_MESSAGES_DEBUG=arkha
Xvfb :71 -screen 0 1200x800x24 -nolisten tcp >/dev/null 2>&1 &
XP=$!
sleep 1
read -r X1 Y1 X2 Y2 <<< "$DRAG"
WM=
if [ "$backend" = wayland ]; then
    weston --backend=x11 --shell=kiosk --width=1200 --height=800 --socket=wl-lockbar --idle-time=0 > "$OUT/weston.log" 2>&1 &
    WM=$!
    sleep 2
    export WAYLAND_DISPLAY=wl-lockbar GDK_BACKEND=wayland
else
    if command -v xmonad >/dev/null; then
        xmonad >/dev/null 2>&1 &
        WM=$!
        sleep 1.5
    fi
    export GDK_BACKEND=x11
fi

ORDER='bar_order=path;ops;split;view;zoom;info;'
order() { grep -h '^bar_order' "$CFG/arkha/arkha.conf" 2>/dev/null; }
# Fresh settings: the default order, and the lock as given.
settings() { rm -rf "$CFG"; mkdir -p "$CFG/arkha"; printf '[General]\n%s\nlock_bar=%s\n' "$ORDER" "$1" > "$CFG/arkha/arkha.conf"; }
drags() { grep -c 'bar drag started' "$LOG"; }

drag() {
    xdotool mousemove "$X1" "$Y1"; sleep 0.3; xdotool mousedown 1; sleep 0.2
    for i in $(seq 1 20); do
        xdotool mousemove $((X1 + (X2 - X1) * i / 20)) $((Y1 + (Y2 - Y1) * i / 20)); sleep 0.05
    done
    for d in 3 -3 0; do xdotool mousemove_relative -- "$d" 0; sleep 0.15; done
    xdotool mouseup 1; sleep 1
}

# View → Lock Toolbar: View clicked, then Up (which goes to the menu's last
# item, Lock Toolbar, wherever the font puts it) and Return; or both
# clicked where MENU says.
toggle_from_menu() {
    if [ -n "$MENU" ]; then
        read -r MX MY LX LY <<< "$MENU"
        xdotool mousemove "$MX" "$MY" click 1; sleep 0.8
        xdotool mousemove "$LX" "$LY" click 1; sleep 0.8
    else
        xdotool mousemove 110 15 click 1; sleep 0.8
        xdotool key Up; sleep 0.4
        xdotool key Return; sleep 0.8
    fi
}

start() {
    : > "$LOG"
    "$BIN" /usr/share >> "$LOG" 2>&1 &
    P=$!
    sleep 3
    [ "$backend" = wayland ] && xdotool search --classname weston windowfocus --sync
    xdotool mousemove 600 450; sleep 0.3
}

stop() { kill $P 2>/dev/null; wait $P 2>/dev/null; }

check() {  # check NAME WANT_DRAG(yes|no) BEFORE_DRAGS BEFORE_ORDER
    local got=no moved=no
    [ "$(drags)" -gt "$3" ] && got=yes
    [ "$(order)" != "$4" ] && moved=yes
    local ok=yes
    [ $got = "$2" ] && [ $moved = "$2" ] || ok=no
    if [ $ok = yes ]; then
        echo "  PASS  $backend: $1 (drag started: $got, bar changed: $moved)"
    else
        echo "  FAIL  $backend: $1 (drag started: $got, bar changed: $moved, wanted: $2)"
        import -window root "$OUT/$backend-$(echo "$1" | tr ' ' '-').png" 2>/dev/null
        fails=$((fails + 1))
    fi
}

one_drag() {  # one_drag NAME LOCK WANT
    settings "$2"
    start
    local d o
    d=$(drags); o=$(order)
    drag
    check "$1" "$3" "$d" "$o"
    stop
}

echo "$backend:"
one_drag "unlocked" false yes
one_drag "locked" true no

# Locking and unlocking from the menu while running.
settings false
start
toggle_from_menu
if grep -q '^lock_bar=true' "$CFG/arkha/arkha.conf"; then
    echo "  PASS  $backend: View → Lock Toolbar saves the lock"
else
    echo "  FAIL  $backend: View → Lock Toolbar didn't save the lock"; fails=$((fails + 1))
fi
d=$(drags); o=$(order); drag; check "locked from the menu" no "$d" "$o"
toggle_from_menu
d=$(drags); o=$(order); drag; check "unlocked from the menu" yes "$d" "$o"
stop

# Anything GTK complained about.
if grep -E 'CRITICAL|WARNING' "$LOG"; then
    echo "  FAIL  $backend: warnings in the log"; fails=$((fails + 1))
fi

[ -n "$WM" ] && kill $WM 2>/dev/null
kill $XP 2>/dev/null
wait 2>/dev/null
exit $((fails > 0))
