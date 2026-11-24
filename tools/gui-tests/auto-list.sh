#!/bin/bash
# auto-list.sh x11|wayland: an icon view with more than 20,000 items shows
# them as a list instead (GtkIconView lays out every item after each
# change: seconds at 100,000), says so in the status bar, and goes back to
# icons below 10,000. The list isn't remembered for the folder. Icons picked
# while there are many stay; another folder or search starts afresh.
GT_A11Y=1 GT_CONF='default_view=icon'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

B=$HOME/big; mkdir -p "$B" "$HOME/small"; : > "$HOME/small/one.txt"
make() { python3 -I -c 'import sys
for i in range(int(sys.argv[2]), int(sys.argv[3])): open(f"{sys.argv[1]}/file {i}.txt", "w").close()' "$B" "$1" "$2"; }
make 10000 35000
# The view in use (the window's view-mode action) and the status bar.
view() { gdbus call --session --dest $APP_ID --object-path "$APP_PATH/window/1" --method org.gtk.Actions.Describe view-mode | grep -o "<'[a-z]*'>" | tr -d "<>'"; }
status() { gt_a11y window-labels | grep -E '^[0-9]+ items|^Loading' | head -1 | sed 's/, Free space: .*//'; }
note() { status | grep -q 'shown as a list' && echo note || echo "no note"; }
remembered() { grep -A3 -F "[$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).as_uri())' "$B")]" "$XDG_CONFIG_HOME/arkha/folders.conf" 2>/dev/null | grep -c '^view=' || true; }
gt_display
gt_arkha "$B"; sleep 4; gt_quiet
gt_shot many
gt_check "25,000 items: a list, with the note" "$(view) $(note)" "list note"
gt_check "25,000 items: the status bar" "$(status)" "25000 items — shown as a list: too many for the icon view"
gt_check "nothing remembered for the folder" "$(remembered)" "0"
# Fewer than 10,000: icons again.
(cd "$B" && find . -name 'file 1[0-9]*.txt' -delete && find . -name 'file 2[0-5]*.txt' -delete)
sleep 4; gt_quiet
gt_check "9,000 items: icons again, no note" "$(ls "$B" | wc -l) $(view) $(note)" "9000 icon no note"
# Many again: a list; icons picked then stay, more arriving too.
make 40000 56000; sleep 4; gt_quiet
gt_check "25,000 again: a list" "$(view)" "list"
gt_act view-mode "<'icon'>"; sleep 2; gt_quiet
make 56000 58000; sleep 3; gt_quiet
gt_check "icons picked while there are many: they stay" "$(view) $(note)" "icon no note"
# Another folder: its own view.
gt_go "$HOME/small" 1; sleep 1; gt_quiet
gt_check "a small folder: icons" "$(view)" "icon"
# A search with many results: a list; a new one with few: icons.
gt_go "$HOME" 1; sleep 1
gt_key ctrl+f; gt_type file; gt_key Return; sleep 6; gt_quiet
gt_shot search
gt_check "a search finding 27,000: a list, with the note" "$(view) $(note)" "list note"
gt_key ctrl+f; gt_key ctrl+a; gt_type 'one'; gt_key Return; sleep 3; gt_quiet
gt_check "a new search finding one: icons" "$(view) $(note)" "icon no note"
gt_quit
gt_end
