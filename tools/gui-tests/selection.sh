#!/bin/bash
# selection.sh x11|wayland: after select all, copy and paste in the same
# folder, exactly the copies are selected (Delete then trashes them and
# nothing else), with 50 files and with 1,500.
. "$(dirname "$0")/lib.sh"
gt_session "$@"

TRASH=$HOME/.local/share/Trash/files
# count DIR [FIND TESTS]: the items in DIR (not below) that pass them.
count() { local d=$1; shift; find "$d" -mindepth 1 -maxdepth 1 "$@" 2>/dev/null | wc -l; }
gt_display
for n in 50 1500; do
  T=$HOME/sel$n; mkdir -p "$T"; (cd "$T" && for i in $(seq 1 $n); do : > "file$i.txt"; done)
  gt_arkha "$T"
  gt_click 900 650
  gt_key ctrl+a; gt_key ctrl+c; gt_key ctrl+v
  for _ in $(seq 60); do [ "$(ls "$T" | wc -l)" -ge $((2*n)) ] && break; sleep 0.5; done; gt_quiet
  gt_shot "pasted-$n"
  gt_key Delete; sleep 0.5; gt_key Return     # "Move to the trash?" (asked by default)
  for _ in $(seq 60); do [ "$(ls "$T" | wc -l)" -le $n ] && break; sleep 0.5; done; gt_quiet
  gt_check "$n files: the copies trashed, the originals left" "$(count "$T" ! -name '*copy*') left, $(count "$TRASH" -name '*copy*') copies in the trash, $(count "$TRASH" ! -name '*copy*') others" "$n left, $n copies in the trash, 0 others"
  rm -rf "$TRASH"; gt_quit
done
gt_end
