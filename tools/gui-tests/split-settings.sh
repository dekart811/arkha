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
CONF=$XDG_CONFIG_HOME/arkha/arkha.conf
title() { sleep 0.8; gt_a11y windows | head -1; }
go() { gt_key ctrl+l; gt_type "$1"; gt_key Return; sleep 0.8; }
# set KEY VALUE: in the settings file, Arkha closed.
set() { grep -q "^$1=" "$CONF" && sed -i "s|^$1=.*|$1=$2|" "$CONF" || sed -i "/^\[General\]/a $1=$2" "$CONF"; }

gt_display
gt_arkha "$HOME/A"
gt_act split-view
gt_check "the extra pane opens where the pane in use is (default)" "$(title)" "A"
go "$HOME/B"
gt_act split-view
gt_check "turned off: the pane in use stays (default)" "$(title)" "B"
gt_quit

set split_keep other
gt_arkha "$HOME/A"
gt_act split-view; go "$HOME/B"
gt_act split-view
gt_check "keep the other pane: the first one stays" "$(title)" "A"
gt_quit

set split_opens home
gt_arkha "$HOME/A"
gt_act split-view
gt_check "opens in the home folder" "$(title)" "Home"
gt_act split-view
gt_quit

set split_opens folder; set split_folder "file://$HOME/C"
gt_arkha "$HOME/A"
gt_act split-view
gt_check "opens in the folder chosen" "$(title)" "C"
go "$HOME/B"
# Kept: the other pane (A); B, the closed one's folder, is remembered.
gt_act split-view
gt_quit

set split_opens last
gt_arkha "$HOME/A"
gt_act split-view
gt_check "opens where the closed pane last was" "$(title)" "B"
# A tab opened and closed, accessibility asked in between and after.
gt_key ctrl+t; sleep 0.5; title > /dev/null
gt_key ctrl+w; sleep 0.5
gt_check "a tab closed: the one before is shown" "$(title)" "B"

# The two choices on Preferences' Behavior page.
gdbus call --session --dest $APP_ID --object-path $APP_PATH/window/1 \
  --method org.gtk.Actions.Activate preferences [] {} >/dev/null & sleep 2
y=180; [ "$BACKEND" = wayland ] && y=194
gt_click 240 $y; gt_key Down; sleep 0.5
gt_shot behavior
gt_check "Preferences offers both" "$(gt_a11y labels | grep -cE '^(When closing split view, keep:|The extra pane opens in:)$')" "2"
gt_end
