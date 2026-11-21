#!/bin/bash
# folder-gone.sh x11|wayland: the open folder deleted or renamed by another
# program: the tab goes to the nearest folder still there, or follows the
# rename (it stayed where nothing was any more).
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/a/b/c" "$HOME/old name"; touch "$HOME/a/b/c/file.txt"
gt_display
gt_arkha "$HOME/a/b/c"
gt_check "showing c" "$(gt_title)" "c"
rm -rf "$HOME/a/b"; sleep 2
gt_check "c deleted (with b): the tab shows a" "$(gt_title)" "a"
gt_go "$HOME/old name" 1
mv "$HOME/old name" "$HOME/new name"; sleep 2
gt_shot renamed
case "$(gt_title)" in
  "new name") echo "PASS renamed: the tab follows it (to new name)" >> "$OUT/result" ;;
  # (The home folder's title: "Home". The system doesn't tell where a
  # renamed open folder went.)
  Home|"$(basename "$HOME")") echo "PASS renamed: the tab goes to the folder above" >> "$OUT/result" ;;
  *) gt_check "renamed: the tab follows or goes above" "$(gt_title)" "new name" ;;
esac
gt_end
