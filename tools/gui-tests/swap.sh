#!/bin/bash
# swap.sh x11|wayland: swapping the two panes of split view. Left shows
# "left" and "left2", right "right" and "more"; swapped with View →
# Swap Panes (its action), with Ctrl+U, and with the toolbar's button, each
# time the tabs change sides and the pane in use keeps the focus; closing
# split view then keeps the pane in use, as before.
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/left" "$HOME/left2" "$HOME/right" "$HOME/more"
gt_display
gt_arkha "$HOME/left"
gt_act new-tab; gt_go "$HOME/left2" 0; gt_key ctrl+Page_Up
tabs() { gt_a11y tabs | tr '\n' ' '; }
gt_ok "no Swap Panes button without split view" sh -c '! python3 "$0" where Swap' "$GT_DIR/a11y.py"
gt_act split-view
gt_go "$HOME/right" 0
gt_act new-tab; gt_go "$HOME/more" 0; gt_key ctrl+Page_Up
gt_shot split
gt_check "split view: left's two tabs, then right's" "$(tabs)" "left left2 right more "
gt_check "the right pane in use" "$(gt_title)" "right"
gt_act swap-panes; gt_shot swapped-action
gt_check "View → Swap Panes: the sides swapped" "$(tabs)" "right more left left2 "
gt_check "the same pane still in use" "$(gt_title)" "right"
gt_key ctrl+u; gt_shot swapped-key
gt_check "Ctrl+U: back" "$(tabs)" "left left2 right more "
xy=$(gt_a11y where Swap)
gt_ok "the toolbar's Swap Panes button" test -n "$xy"
[ -n "$xy" ] && gt_click $xy; gt_shot swapped-button
gt_check "the button: swapped again" "$(tabs)" "right more left left2 "
gt_key F6
gt_check "F6 still goes to the other pane" "$(gt_title)" "left"
gt_key F6
gt_act split-view; gt_shot closed
gt_check "closing split view keeps the pane in use" "$(tabs)" "right more "
gt_end
