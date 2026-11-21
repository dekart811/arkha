#!/bin/bash
# search-paste.sh x11|wayland: files cut from search results paste with
# Ctrl+V in another tab. The search runs in one tab, its results are cut,
# then the other tab (a folder) is chosen and Ctrl+V pressed: the files
# have to move there. (Switching tabs set the actions as the tab left
# allowed them: the search results' Paste stayed off.) Ctrl+V in the
# results themselves still pastes nothing, as in Caja. Then the results
# follow the move: files moved out of the folder searched leave them, a
# file moved to a folder within it stays, at its new place (they have no
# folder monitor to tell them).
GT_A11Y=1 GT_CONF='default_view=list'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/Downloads/old" "$HOME/Downloads/archive" "$HOME/sorted"
touch "$HOME/Downloads/report-1.txt" "$HOME/Downloads/old/report-2.txt" "$HOME/Downloads/notes.txt" "$HOME/Downloads/invoice.txt"
gt_display
gt_arkha "$HOME/sorted"
# A second tab with Downloads, searched for "report".
gt_key ctrl+t; gt_go "$HOME/Downloads" 0.5
gt_key ctrl+f; gt_type report; gt_key Return; sleep 2
gt_shot results
# The results get the focus (the search field has it), all of them are cut.
gt_click 500 270; gt_key ctrl+a; gt_key ctrl+x
gt_key ctrl+v; sleep 1
gt_check "Ctrl+V in the search results pastes nothing" "$(ls "$HOME/sorted" | wc -l)" "0"
# The first tab, then Ctrl+V.
gt_key ctrl+Page_Up; sleep 0.5
gt_key ctrl+v
for _ in $(seq 1 30); do [ "$(ls "$HOME/sorted" | wc -l)" = 2 ] && break; sleep 0.2; done
gt_shot pasted
gt_check "Ctrl+V in the other tab moves the cut results there" "$(ls "$HOME/sorted" | tr '\n' ' ')" "report-1.txt report-2.txt "
gt_check "and they're gone from where they were" "$(find "$HOME/Downloads" -name 'report-*' | wc -l)" "0"
# The results' names (the list's cells: name, size, type, location, …).
results() { gt_a11y cells | grep -E '^(report|invoice)' | tr '\n' ' '; }
gt_key ctrl+Page_Down; sleep 1
gt_shot results-after
gt_check "the moved files left the search results" "$(results)" ""
# Searched again, for a file then moved to a folder in Downloads.
gt_key ctrl+f; gt_key ctrl+a; gt_type invoice; gt_key Return; sleep 2
gt_check "the new search finds it" "$(results)" "invoice.txt "
gt_click 500 270; gt_key ctrl+a; gt_key ctrl+x
gt_key ctrl+Page_Up; gt_go "$HOME/Downloads/archive" 0.5
gt_key ctrl+v
for _ in $(seq 1 30); do [ -e "$HOME/Downloads/archive/invoice.txt" ] && break; sleep 0.2; done
gt_key ctrl+Page_Down; sleep 1
gt_shot results-moved-within
gt_check "moved within Downloads, it's still found" "$(results)" "invoice.txt "
gt_check "at its new place" "$(gt_a11y cells | grep -c '/archive$\|^archive$\|Downloads/archive')" "1"
gt_end
