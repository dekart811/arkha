#!/bin/bash
# thumb-samename.sh x11|wayland: a file in place of one of the same name
# shows its own thumbnail. A red folder.jpg in A is shown (its thumbnail
# made and loaded), then trashed; a blue folder.jpg from B is moved into
# A, its thumbnail with it, under the same name as the red one's: A has
# to show the blue one (the red picture was still loaded, by that name).
GT_CONF='show_thumbnails=always'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/A" "$HOME/B"
magick -size 200x200 xc:red "$HOME/A/folder.jpg"
magick -size 200x200 xc:blue "$HOME/B/folder.jpg"
gt_display
gt_arkha "$HOME/A"; sleep 2; gt_quiet
gt_go "$HOME/B" 2
gt_go "$HOME/A" 2
gt_click 600 400; gt_key Home; gt_key Delete; sleep 0.5; gt_key Return; sleep 1
gt_go "$HOME/B" 2
gt_click 600 400; gt_key Home; gt_key ctrl+x
gt_go "$HOME/A" 2
gt_key ctrl+v; sleep 2; gt_quiet
gt_shot after
# The colour of the item's picture: its middle, by its red and blue.
colour() { magick "$OUT/after.png" -crop 30x20+245+125 -scale 1x1\! -format '%[fx:int(255*r)] %[fx:int(255*b)]' info:; }
read -r r b <<< "$(colour)"
gt_check "the moved-in folder.jpg shows its own picture" "$([ "$b" -gt 150 ] && [ "$r" -lt 100 ] && echo blue || echo "not blue (red $r, blue $b)")" "blue"
gt_end
