#!/bin/bash
# arrive-big.sh x11|wayland: 4,000 files arriving at once in a folder of
# 20,000 shown in the icon view keep the window answering, sorted by name
# and sorted by owner (a detail read only when chosen). Each row's chosen
# details were set apart from the rest, a second change of each row, and
# GtkIconView measures all its items again on any row changing: sorted by
# owner, Arkha didn't answer for over 25 s (a soak run found it).
# Answering is asked over D-Bus (an action list), not through
# accessibility, whose own bus lags behind thousands of new items. A call
# that fails (no Arkha, or gdbus gave up after its 25 s) is no answer,
# however soon it returns.
# Many rows are added with the view detached: the item selected before
# stays selected (Delete trashes it and nothing else), and a rename begun
# before isn't disturbed. Nor does the view move a pixel or lose its
# keyboard cursor: in the middle, at the top with the selected item out of
# sight, and in the compact view.
GT_CONF='default_view=icon'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

gt_display
for sort in name owner; do
  rm -rf "$HOME/big"; mkdir -p "$HOME/big"
  (cd "$HOME/big" && for i in $(seq 10000 29999); do : > "file $i.txt"; done)
  printf '[General]\ndefault_view=icon\nsort_by=%s\n' "$sort" > "$XDG_CONFIG_HOME/arkha/arkha.conf"
  gt_arkha "$HOME/big"; sleep 2; gt_quiet
  gt_click 600 400; gt_type 'file 10007'; gt_key Escape
  (cd "$HOME/big" && for i in $(seq 1 4000); do : > "new $i.txt"; done)
  # The longest wait for an answer while they come in.
  most=0; failed=0; start=$(date +%s%N)
  while [ $(( ($(date +%s%N) - start) / 1000000000 )) -lt 12 ]; do
    t=$(date +%s%N)
    timeout 60 gdbus call --session --dest $APP_ID --object-path "$APP_PATH/window/1" --method org.gtk.Actions.List > /dev/null || failed=$((failed + 1))
    ms=$(( ($(date +%s%N) - t) / 1000000 ))
    [ $ms -gt $most ] && most=$ms
    sleep 0.2
  done
  gt_note "sorted by $sort: slowest answer $most ms, $failed calls unanswered"
  gt_shot "$sort"
  gt_check "sorted by $sort: answered within 5 s throughout" "$([ $most -lt 5000 ] && [ $failed = 0 ] && echo yes || echo "no, $most ms, $failed unanswered")" "yes"
  rm -rf "$HOME/.local/share/Trash"
  gt_key Delete; sleep 0.5; gt_key Return; sleep 2     # "Move to the trash?"
  gt_check "sorted by $sort: the selection kept" "$(find "$HOME/.local/share/Trash/files" -mindepth 1 -printf '%f;' 2>/dev/null)" "file 10007.txt;"
  gt_quit
done

rm -rf "$HOME/big"; mkdir -p "$HOME/big"
(cd "$HOME/big" && for i in $(seq 10000 14999); do : > "file $i.txt"; done)
gt_arkha "$HOME/big"; sleep 2; gt_quiet
gt_click 600 400; gt_type 'file 10007'; gt_key Escape
gt_key F2
(cd "$HOME/big" && for i in $(seq 1 1000); do : > "new $i.txt"; done)
sleep 6; gt_quiet
gt_key ctrl+a; gt_type renamed.txt; gt_key Return; sleep 2
gt_check "a rename begun before they came in" "$([ -e "$HOME/big/renamed.txt" ] && [ ! -e "$HOME/big/file 10007.txt" ] && echo renamed)" "renamed"
gt_quit
for case in middle top compact; do
  rm -rf "$HOME/big"; mkdir -p "$HOME/big"
  (cd "$HOME/big" && for i in $(seq 10000 14999); do : > "file $i.txt"; done)
  view=icon at=12500
  [ $case = top ] && at=10080
  [ $case = compact ] && view=compact
  printf '[General]\ndefault_view=%s\nper_folder=false\n' "$view" > "$XDG_CONFIG_HOME/arkha/arkha.conf"
  gt_arkha "$HOME/big"; sleep 2; gt_quiet
  gt_click 600 400; gt_type "file $at"; gt_key Escape
  if [ $case = top ]; then
    xdotool mousemove 600 400
    for _ in $(seq 40); do xdotool click 4; sleep 0.03; done
  fi
  xdotool mousemove 600 15; sleep 1.5; gt_quiet
  gt_shot "$case-before"
  (cd "$HOME/big" && for i in $(seq 1 1000); do : > "new $i.txt"; done)
  sleep 5; gt_quiet
  gt_shot "$case-after"
  # The files shown, without the scroll bars and the status bar.
  for n in before after; do magick "$OUT/$case-$n.png" -crop 975x590+197+150 +repage "$OUT/$case-$n-view.png"; done
  gt_check "$case: the view as before (pixels differing)" "$(magick compare -metric AE "$OUT/$case-before-view.png" "$OUT/$case-after-view.png" null: 2>&1 | cut -d' ' -f1)" "0"
  if [ $case != compact ]; then
    gt_key Right; gt_key F2; gt_key ctrl+a; gt_type renamed.txt; gt_key Return; sleep 2
    gt_check "$case: the cursor kept (Right, then rename)" "$(for i in $(seq 10000 14999); do [ -e "$HOME/big/file $i.txt" ] || echo "$i"; done)" "$((at + 1))"
  fi
  gt_quit
done
gt_end
