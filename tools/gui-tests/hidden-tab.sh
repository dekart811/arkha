#!/bin/bash
# hidden-tab.sh x11|wayland: tabs not shown, their icon views on a big
# folder, don't hold the window up when its size changes (the side pane
# shown and hidden; under a tiling window manager, any window opening or
# closing). GtkIconView laid each hidden view out again, and with one item
# a row (zoomed in) that cost ~0.5 s for 5,000 items (here 8,000): hidden tabs like that
# kept Arkha from answering for over 30 s in soak runs. Shown again, a tab
# has every item, one that came while it was hidden too.
GT_CONF='default_view=icon'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/big" "$HOME/other"
(cd "$HOME/big" && for i in $(seq 10000 17999); do : > "file $i.txt"; done)
gt_display
gt_arkha "$HOME/big"; sleep 1; gt_quiet
for _ in 1 2 3 4 5 6; do gt_act zoom-in; done
sleep 1; gt_quiet
for _ in 1 2; do gt_act new-tab; gt_go "$HOME/big" 1; sleep 1; gt_quiet; done
gt_act new-tab; gt_go "$HOME/other" 1; sleep 1; gt_quiet
gt_shot other-tab
# Files come and go in the hidden tab's folder, as in a download or a copy.
(
  cd "$HOME/big" || exit
  for round in $(seq 1 40); do
    for i in $(seq 1 10); do : > "new $round-$i.txt"; done
    sleep 0.25
    rm -f "new $round-"*.txt
    sleep 0.25
  done
) & changer=$!
# Meanwhile the window's file area changes size, as a tiling window
# manager does to every window when another one opens or closes.
(
  for _ in $(seq 1 20); do
    timeout 60 gdbus call --session --dest $APP_ID --object-path "$APP_PATH/window/1" --method org.gtk.Actions.Activate show-sidebar '[]' '{}' > /dev/null
    sleep 0.5
  done
) &
most=0; failed=0
while kill -0 $changer 2>/dev/null; do
  t=$(date +%s%N)
  timeout 60 gdbus call --session --dest $APP_ID --object-path "$APP_PATH/window/1" --method org.gtk.Actions.List > /dev/null || failed=$((failed + 1))
  ms=$(( ($(date +%s%N) - t) / 1000000 )); [ $ms -gt $most ] && most=$ms
  sleep 0.2
done
gt_note "hidden tabs, size changing: slowest answer $most ms, $failed unanswered"
gt_check "the window keeps answering (within 0.4 s)" "$([ $most -lt 400 ] && [ $failed = 0 ] && echo yes || echo "no, $most ms, $failed unanswered")" "yes"
: > "$HOME/big/arrived while hidden.txt"; sleep 2
gt_act prev-tab; gt_act prev-tab; gt_act prev-tab; sleep 2; gt_quiet
gt_shot big-tab
gt_go "$HOME/big" 1
gt_click 600 400; gt_type 'arrived while'; gt_key Escape
gt_key F2; gt_key ctrl+a; gt_type renamed.txt; gt_key Return; sleep 2
gt_check "shown again: the file that came meanwhile is there" "$([ -e "$HOME/big/renamed.txt" ] && echo renamed)" "renamed"
gt_quit
gt_end
