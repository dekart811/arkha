#!/bin/bash
# previews.sh x11|wayland: the preview settings. Thumbnails always, with a
# size limit of 1 MB (the small pictures get one, the big one doesn't), or
# never (none); folders' item counts always ("2 items" in the Size column)
# or never ("—").
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

T=$HOME/pics; mkdir -p "$T/folder"; touch "$T/folder/x" "$T/folder/y"
for c in red green blue; do magick -size 200x150 "xc:$c" "$T/$c.png"; done
magick -size 1000x1000 xc: +noise Random "$T/big.png"     # about 3 MB
conf() { printf '[General]\nper_folder=false\n%s\n' "$1" > "$XDG_CONFIG_HOME/arkha/arkha.conf"; rm -rf "$HOME/.cache/thumbnails"; }
gt_display
conf 'show_thumbnails=always
thumbnail_limit=1048576'
gt_arkha "$T"; sleep 3; gt_quiet; gt_shot always
gt_check "thumbnails always, up to 1 MB: the three small pictures" "$(gt_thumbs)" "3"
gt_quit
conf 'show_thumbnails=never'
gt_arkha "$T"; sleep 3; gt_quiet; gt_shot never
gt_check "thumbnails never" "$(gt_thumbs)" "0"
gt_quit
conf 'default_view=list
count_items=always'
gt_arkha "$T"; gt_shot counted
gt_check "item counts always" "$(gt_a11y cells | grep -c '^2 items$')" "1"
gt_quit
conf 'default_view=list
count_items=never'
gt_arkha "$T"; gt_shot not-counted
gt_check "item counts never" "$(gt_a11y cells | grep -c '^—$')" "1"
gt_end
