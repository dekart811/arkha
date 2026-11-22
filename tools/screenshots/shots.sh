#!/bin/bash
# shots.sh x11 light|dark: one theme's screenshots, for take.sh (which makes
# the home folder first), in target/gui-tests/screenshots-<theme>-x11/:
# icons.png (the icon view with the info panel), split.png (split view with
# tabs, one pane in the list view), rename.png (Rename All) and
# checksums.png (a picture's Properties, its checksums).
THEME=${2:-light}
GT_NAME=screenshots-$THEME
GT_WM=none
GT_A11Y=1
. "$(dirname "$0")/../gui-tests/lib.sh"
gt_session "$1" "$THEME"
HERE=$GT_ROOT/tools/screenshots
H=$HOME
cp -a "$GT_ROOT/target/screenshots/home/." "$H/"
mkdir -p "$XDG_CONFIG_HOME/gtk-3.0"
if [ "$THEME" = dark ]; then t=Adwaita-dark i=Papirus-Dark; else t=Adwaita i=Papirus; fi
printf '[Settings]\ngtk-theme-name=%s\ngtk-icon-theme-name=%s\ngtk-font-name=Cantarell 11\n' "$t" "$i" > "$XDG_CONFIG_HOME/gtk-3.0/settings.ini"
printf 'file://%s/Documents\nfile://%s/Music\nfile://%s/Pictures\nfile://%s/Projects\n' "$H" "$H" "$H" "$H" > "$XDG_CONFIG_HOME/gtk-3.0/bookmarks"
# English dates and numbers too, whatever this machine's locale.
export LC_ALL=en_US.UTF-8

# Wide enough for a dialog beside the main window, each cut out on its own.
gt_display 2800 1100
# Metacity: title bars from the GTK theme (Adwaita), click to focus.
metacity --replace > "$OUT/metacity.log" 2>&1 & GT_PIDS="$GT_PIDS $!"; sleep 2
# The backdrop behind the windows: white, black while $FLAG exists.
FLAG=$OUT/backdrop-black
rm -f "$FLAG"
LC_ALL=C python3 "$HERE/backdrop.py" "$FLAG" > "$OUT/backdrop.log" 2>&1 & GT_PIDS="$GT_PIDS $!"; sleep 1.5

# conf WIDTH HEIGHT: Arkha's window size for the next start.
conf() { printf '[General]\nwidth=%s\nheight=%s\n' "$1" "$2" > "$XDG_CONFIG_HOME/arkha/arkha.conf"; }
# top TITLE: Arkha's window with TITLE in its title.
top() { xdotool search --onlyvisible --pid "$ARKHA_PID" --name "$1" 2>/dev/null | tail -1; }
# shot NAME WINDOW: the window with its title bar and shadow, cut from the
# screen on white and on black, made into NAME.png with transparency.
shot() {
  local X Y WIDTH HEIGHT x y w h
  sleep 1.5
  eval "$(xdotool getwindowgeometry --shell "$2")"
  x=$((X - 80)); y=$((Y - 120)); [ $x -lt 0 ] && x=0; [ $y -lt 0 ] && y=0
  w=$((WIDTH + 160)); h=$((HEIGHT + 200))
  import -window root -crop "${w}x${h}+${x}+${y}" +repage "$OUT/$1-on-white.png" 2>/dev/null
  touch "$FLAG"; sleep 0.8
  import -window root -crop "${w}x${h}+${x}+${y}" +repage "$OUT/$1-on-black.png" 2>/dev/null
  rm -f "$FLAG"; sleep 0.5
  sh "$HERE/alpha.sh" "$OUT/$1-on-white.png" "$OUT/$1-on-black.png" "$OUT/$1.png"
  gt_ok "$1.png taken" test -s "$OUT/$1.png"
}
where() { python3 "$GT_DIR/a11y.py" "$@"; }
# start ARGS: Arkha, with the pointer out of the way (no hover effects).
start() { gt_arkha "$@"; xdotool mousemove 2790 1090; }
# apart TITLE DIALOG: the main window to the left, the dialog to the right.
apart() { xdotool windowmove "$(top "$1")" 40 60; sleep 0.5; xdotool windowmove "$2" 1600 150; sleep 1; }

# The icon view one size up, the info panel, a picture selected.
conf 1180 720
start "$H/Pictures"
gt_act zoom-in
gt_act info-panel
gt_type "Lake"; sleep 6; gt_quiet
shot icons "$(top Pictures)"
gt_quit

# Split view: home in the icon view, an album in the list view; two tabs each.
conf 1420 720
start "$H"
gt_act new-tab; gt_key ctrl+l; gt_type "$H/Downloads"; gt_key Return; gt_key ctrl+Page_Up
gt_act split-view
gt_key ctrl+l; gt_type "$H/Music/Selma Hartwig - Northern Lights"; gt_key Return
gt_act view-mode "<'list'>"
gt_act new-tab; gt_key ctrl+l; gt_type "$H/Projects"; gt_key Return; gt_key ctrl+Page_Up
gt_type "03"; sleep 6; gt_quiet
shot split "$(top "Selma")"
gt_quit

# Rename All on the holiday pictures: IMG_ replaced by "Holiday ".
conf 1000 640
start "$H/Pictures/Holiday"
gt_key ctrl+a
gt_act rename-all
apart Holiday "$(top Rename)"
xy=$(where where-tab Replace); [ -n "$xy" ] && gt_click "${xy% *}" "${xy#* }"
gt_key Tab; gt_type "IMG_"; gt_key Tab; gt_type "Holiday "
gt_quiet
shot rename "$(top Rename)"
gt_quit

# A picture's Properties: its checksums (the Basic tab would show this
# machine's paths, Permissions its user name).
conf 1000 640
start "$H/Pictures"
gt_type "Lake"; sleep 6
gt_act properties
p=$(top Properties)
apart Pictures "$p"
xy=$(where where-tab Checksums); [ -n "$xy" ] && gt_click "${xy% *}" "${xy#* }" && sleep 1
where press "Calculate All"; sleep 3
shot checksums "$p"
gt_quit
gt_end
