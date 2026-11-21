#!/bin/bash
# undo.sh x11|wayland: with real keys, a copy undone (the copies go to the
# trash), a move undone (moved back), and Ctrl+Z in the location bar, which
# must undo nothing (it's the text field's).
. "$(dirname "$0")/lib.sh"
gt_session "$@"

T=$HOME/undo; TRASH=$HOME/.local/share/Trash/files
mkdir -p "$T/src/sub" "$T/dest"; echo a > "$T/src/a.txt"; echo c > "$T/src/sub/c"
state() { echo "src: $(cd "$T/src" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')| dest: $(cd "$T/dest" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')| trash: $(ls "$TRASH" 2>/dev/null | tr '\n' ' ')"; }
gt_display
gt_arkha "$T/src"
gt_click 900 650
gt_key ctrl+a; gt_key ctrl+c; gt_key ctrl+v; sleep 1
gt_check "copied" "$(state)" "src: ./a (copy).txt ./a.txt ./sub ./sub (copy) ./sub (copy)/c ./sub/c | dest: | trash: "
gt_key ctrl+z; sleep 1
gt_check "the copy undone: the copies in the trash" "$(state)" "src: ./a.txt ./sub ./sub/c | dest: | trash: a (copy).txt sub (copy) "
gt_key ctrl+a; gt_key ctrl+x; gt_go "$T/dest" 0; gt_key ctrl+v; sleep 1
gt_check "moved" "$(state)" "src: | dest: ./a.txt ./sub ./sub/c | trash: a (copy).txt sub (copy) "
gt_key ctrl+l; gt_key ctrl+z
gt_check "Ctrl+Z in the location bar undoes nothing" "$(state)" "src: | dest: ./a.txt ./sub ./sub/c | trash: a (copy).txt sub (copy) "
gt_key Escape; gt_key ctrl+z; sleep 1
gt_check "the move undone" "$(state)" "src: ./a.txt ./sub ./sub/c | dest: | trash: a (copy).txt sub (copy) "
gt_shot end
gt_end
