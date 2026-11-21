#!/bin/bash
# thumb-retry.sh x11|wayland: a video whose thumbnail failed while it was
# still arriving gets one once it's complete, even if it then has the very
# time it failed with (its copy ended in that second). The failure is
# noted with the file's time and size; with the time alone such a video
# was never tried again.
GT_CONF='show_thumbnails=always'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

V=$HOME/videos; mkdir -p "$V"
ffmpeg -loglevel error -f lavfi -i testsrc=duration=2:size=320x240:rate=15 "$HOME/clip.mp4"
fails() { find "$HOME/.cache/thumbnails/fail" -name '*.png' 2>/dev/null | wc -l; }
# Half of it there: its index (at the end) missing, it can't be read yet.
head -c $(( $(stat -c %s "$HOME/clip.mp4") / 2 )) "$HOME/clip.mp4" > "$V/clip.mp4"
gt_display
gt_arkha "$V"; sleep 3; gt_quiet
gt_shot half
gt_check "the half-written video: no thumbnail, the failure noted" "$(gt_thumbs) $(fails)" "0 1"
# Only for their owner, as the thumbnail spec asks: the notes name files.
mode() { stat -c %a "$1" 2>/dev/null; }
gt_check "the cache and its failure notes private" "$(mode "$HOME/.cache/thumbnails") $(mode "$HOME/.cache/thumbnails/fail") $(mode "$(find "$HOME/.cache/thumbnails/fail" -name '*.png' | head -1)")" "700 700 600"
# Complete now, with the time it had when it failed.
t=$(stat -c %Y "$V/clip.mp4")
cat "$HOME/clip.mp4" > "$V/clip.mp4"; touch -d "@$t" "$V/clip.mp4"
sleep 2; gt_key F5; sleep 3; gt_quiet
gt_shot complete
gt_check "complete, with the same time: its thumbnail is made" "$(gt_thumbs)" "1"
gt_check "the thumbnail private too" "$(mode "$(find "$HOME/.cache/thumbnails" -name '*.png' -not -path '*/fail/*' | head -1)")" "600"
gt_end
