#!/bin/bash
# leave-big.sh x11|wayland: leaving a folder of 20,000 files is quick, in
# the list and the icon view. Emptying the view's rows with GTK's own
# clear took 6–7 s here: it removes the first row each time, and the
# filter model on the rows then moves every row after it (see
# empty_from_end in pane.rs).
GT_A11Y=1 GT_CONF='default_view=list'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/big"
(cd "$HOME/big" && for i in $(seq 10000 29999); do : > "file $i.txt"; done)
gt_display
for view in list icon; do
  printf '[General]\ndefault_view=%s\n' $view > "$XDG_CONFIG_HOME/arkha/arkha.conf"
  gt_arkha "$HOME/big"; sleep 1; gt_quiet
  gt_check "$view view: the folder is open" "$(gt_title)" "big"
  start=$(date +%s%N)
  gt_act go-uri "<'file://$HOME'>"
  ms=$(( ($(date +%s%N) - start) / 1000000 ))
  gt_note "$view view: home shown after $ms ms (at least 400: the wait for quiet)"
  # (The title tells it was left: a refused call would take no time.)
  gt_check "$view view: home is shown" "$(gt_title)" "Home"
  gt_check "$view view: left within 2 s" "$([ $ms -lt 2000 ] && echo yes || echo "no, $ms ms")" "yes"
  gt_shot "$view"
  gt_quit
done
gt_end
