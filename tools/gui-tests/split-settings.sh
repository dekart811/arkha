#!/bin/bash
# split-settings.sh x11|wayland: Preferences → Behavior → Split View. When
# split view is turned off (F3), the pane in use stays (the default) or, as
# set, the other one; the extra pane opens in the folder of the pane in use
# (the default), home, the folder the closed pane last showed, or a folder
# of one's own. The window's title is the folder of the pane in use.
# Accessibility is asked between the steps, as a screen reader would: the
# tabs of a closed pane (or a closed tab) are gone without GTK warnings.
GT_A11Y=1 GT_CONF='default_view=list'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/A" "$HOME/B" "$HOME/C"

gt_display
gt_arkha "$HOME/A"
gt_act split-view
gt_check "the extra pane opens where the pane in use is (default)" "$(gt_title 0.8)" "A"
gt_go "$HOME/B"
gt_act split-view
gt_check "turned off: the pane in use stays (default)" "$(gt_title 0.8)" "B"
gt_quit

gt_conf split_keep other
gt_arkha "$HOME/A"
gt_act split-view; gt_go "$HOME/B"
gt_act split-view
gt_check "keep the other pane: the first one stays" "$(gt_title 0.8)" "A"
gt_quit

gt_conf split_opens home
gt_arkha "$HOME/A"
gt_act split-view
gt_check "opens in the home folder" "$(gt_title 0.8)" "Home"
gt_act split-view
gt_quit

gt_conf split_opens folder; gt_conf split_folder "file://$HOME/C"
gt_arkha "$HOME/A"
gt_act split-view
gt_check "opens in the folder chosen" "$(gt_title 0.8)" "C"
gt_go "$HOME/B"
# Kept: the other pane (A); B, the closed one's folder, is remembered.
gt_act split-view
gt_quit

gt_conf split_opens last
gt_arkha "$HOME/A"
gt_act split-view
gt_check "opens where the closed pane last was" "$(gt_title 0.8)" "B"
# A tab opened and closed, accessibility asked in between and after.
gt_key ctrl+t; sleep 0.5; gt_title > /dev/null
gt_key ctrl+w; sleep 0.5
gt_check "a tab closed: the one before is shown" "$(gt_title 0.8)" "B"

# The two choices on Preferences' Behavior page.
gt_prefs 1
gt_shot behavior
gt_check "Preferences offers both" "$(gt_a11y labels | grep -cE '^(When closing split view, keep:|The extra pane opens in:)$')" "2"
gt_end
