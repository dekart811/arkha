#!/bin/bash
# thumb-move.sh x11|wayland: thumbnails go with their files. A video with
# a thumbnail moved to another folder, then renamed, keeps its thumbnail
# (moved and renamed with it, the URI in it updated) instead of a new one
# being made and the old one staying behind; deleted, its thumbnail goes
# too. Another trashed keeps its thumbnail, also once restored (Undo) and
# trashed again; moved out of the trash to another folder, the thumbnail
# goes with it; emptying the trash removes it.
GT_A11Y=1 GT_CONF='show_thumbnails=always'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/a" "$HOME/b"
ffmpeg -loglevel error -f lavfi -i testsrc=duration=2:size=320x240:rate=15 "$HOME/a/clip.mp4"
cp "$HOME/a/clip.mp4" "$HOME/a/other.mp4"
uri() { python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).as_uri())' "$1"; }
md5() { printf %s "$(uri "$1")" | md5sum | cut -d' ' -f1; }
# The thumbnail kept for the file at $1 (its name from the file's URI),
# and the URI written in it.
thumb_of() {
  local t; t=$(find "$HOME/.cache/thumbnails" -name "$(md5 "$1").png" -not -path '*/fail/*' | head -1)
  [ -n "$t" ] && python3 - "$t" <<'PY'
import struct, sys
b = open(sys.argv[1], "rb").read()[8:]
while len(b) >= 12:
    n, = struct.unpack(">I", b[:4])
    kind, data = b[4:8], b[8:8 + n]
    if kind == b"tEXt" and data.startswith(b"Thumb::URI\0"):
        print(data[11:].decode())
    b = b[12 + n:]
PY
}
made() { grep -c 'thumbnail: running' "$OUT/arkha.log"; }
export G_MESSAGES_DEBUG=arkha
gt_display
gt_arkha "$HOME/a"; sleep 3; gt_quiet
gt_check "both videos get a thumbnail" "$(gt_thumbs) $(made)" "2 2"
# Moved to b.
gt_click 600 400; gt_type clip; gt_key Escape; gt_key ctrl+x
gt_go "$HOME/b" 0.5; gt_key ctrl+v; sleep 3; gt_quiet
gt_shot moved
gt_check "moved: its thumbnail kept under its new URI, none made" "$(thumb_of "$HOME/b/clip.mp4") $(gt_thumbs) $(made)" "$(uri "$HOME/b/clip.mp4") 2 2"
# Renamed there.
gt_key F2; gt_key ctrl+a; gt_type renamed.mp4; gt_key Return; sleep 3; gt_quiet
gt_check "renamed: its thumbnail renamed, none made" "$(thumb_of "$HOME/b/renamed.mp4") $(gt_thumbs) $(made)" "$(uri "$HOME/b/renamed.mp4") 2 2"
# Deleted permanently (Enter confirms).
gt_key shift+Delete; sleep 0.5; gt_key Return; sleep 2
gt_check "deleted: its thumbnail gone" "$([ -e "$HOME/b/renamed.mp4" ] && echo there || echo deleted) $(gt_thumbs)" "deleted 1"
# The other one trashed, then the trash emptied.
gt_go "$HOME/a" 1
gt_click 600 400; gt_type other; gt_key Escape; gt_key Delete; sleep 0.5; gt_key Return; sleep 2
gt_check "trashed: its thumbnail kept (for a restore)" "$([ -e "$HOME/a/other.mp4" ] && echo there || echo trashed) $(gt_thumbs)" "trashed 1"
# Restored (Undo), its thumbnail still there, none made; trashed again.
gt_key ctrl+z; sleep 2; gt_quiet
gt_check "restored: its thumbnail kept, none made" "$(thumb_of "$HOME/a/other.mp4") $(gt_thumbs) $(made)" "$(uri "$HOME/a/other.mp4") 1 2"
gt_click 600 400; gt_type other; gt_key Escape; gt_key Delete; sleep 0.5; gt_key Return; sleep 2
gt_check "trashed again" "$([ -e "$HOME/a/other.mp4" ] && echo there || echo trashed) $(gt_thumbs)" "trashed 1"
# Cut in the Trash and pasted into b: the thumbnail kept for it (under its
# URI before) goes with it, none made; then trashed from there.
gt_go trash:/// 1
gt_click 600 400; gt_type other; gt_key Escape; gt_key ctrl+x
gt_go "$HOME/b" 1; gt_key ctrl+v; sleep 3; gt_quiet
gt_check "out of the trash into b: its thumbnail with it, none made" "$(thumb_of "$HOME/b/other.mp4") $(thumb_of "$HOME/a/other.mp4")$(gt_thumbs) $(made)" "$(uri "$HOME/b/other.mp4") 1 2"
gt_click 600 400; gt_type other; gt_key Escape; gt_key Delete; sleep 0.5; gt_key Return; sleep 2
gt_check "trashed from b" "$([ -e "$HOME/b/other.mp4" ] && echo there || echo trashed) $(gt_thumbs)" "trashed 1"
# Its confirmation holds the action's D-Bus call until it's answered.
gt_act empty-trash & sleep 1.5
gt_ok "Empty Trash confirmed" gt_a11y press "Empty Trash"; sleep 2
gt_check "trash emptied: its thumbnail gone" "$(find "$HOME/.local/share/Trash/files" -mindepth 1 | wc -l) $(gt_thumbs)" "0 0"
gt_end
