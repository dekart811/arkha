#!/bin/bash
# tab-settings.sh x11|wayland: Preferences → Behavior → Tabs → "New tabs
# open in:". New Tab (Ctrl+T) opens the current tab's folder (the
# default), home, the folder of the last closed tab, or a folder of one's
# own; a folder opened in a new tab (Ctrl+Enter) is that folder whatever
# the setting. The folder picked from the folder button's own list (File
# System) is the one stored (it once stayed the one before: only folders
# from its dialog were). The window's title is the current tab's folder.
GT_A11Y=1 GT_CONF='default_view=list'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/A/D" "$HOME/B" "$HOME/C"
CONF=$XDG_CONFIG_HOME/arkha/arkha.conf
title() { sleep 0.8; gt_a11y windows | head -1; }
go() { gt_key ctrl+l; gt_type "$1"; gt_key Return; sleep 0.8; }
# set KEY VALUE: in the settings file, Arkha closed.
set() { grep -q "^$1=" "$CONF" && sed -i "s|^$1=.*|$1=$2|" "$CONF" || sed -i "/^\[General\]/a $1=$2" "$CONF"; }

gt_display
gt_arkha "$HOME/A"
gt_key ctrl+t
gt_check "New Tab: the current tab's folder (default)" "$(title)" "A"
gt_quit

set tab_opens home
gt_arkha "$HOME/A"
gt_key ctrl+t
gt_check "New Tab: the home folder" "$(title)" "Home"
# A folder opened in a new tab is that folder, not home.
gt_key ctrl+w; sleep 0.5
gt_click 300 147; sleep 0.3; gt_click 300 147; sleep 0.3
gt_key ctrl+Return; sleep 1
gt_check "a folder opened in a new tab is shown there" "$(gt_a11y tabs | grep -c '^D$')" "1"
gt_quit

set tab_opens folder; set tab_folder "file://$HOME/C"
gt_arkha "$HOME/A"
gt_key ctrl+t
gt_check "New Tab: the folder chosen" "$(title)" "C"
# A tab in B, closed: B is the last closed tab's folder.
go "$HOME/B"; gt_key ctrl+w
gt_quit

set tab_opens last
gt_arkha "$HOME/A"
gt_key ctrl+t
gt_check "New Tab: the folder of the last closed tab" "$(title)" "B"

# The choice on Preferences' Behavior page.
gdbus call --session --dest $APP_ID --object-path $APP_PATH/window/1 \
  --method org.gtk.Actions.Activate preferences [] {} >/dev/null & sleep 2
y=180; [ "$BACKEND" = wayland ] && y=194
gt_click 240 $y; gt_key Down; sleep 0.5
gt_shot behavior
gt_check "Preferences offers it" "$(gt_a11y labels | grep -c '^New tabs open in:$')" "1"
gt_quit

# "This folder:", File System picked from the button's list.
set tab_opens folder; set tab_folder "file://$HOME/C"
gt_arkha "$HOME/A"
gdbus call --session --dest $APP_ID --object-path $APP_PATH/window/1 \
  --method org.gtk.Actions.Activate preferences [] {} >/dev/null & sleep 2
gt_click 240 $y; gt_key Down; sleep 0.5
dy=$((y - 180))
gt_click 930 $((318 + dy)); sleep 0.8
gt_shot button-list
# The list opens at the folder shown (C, after home, Desktop, File
# System): one up is File System.
gt_key Up; gt_key Return; sleep 0.8
gt_check "File System from the button's list is stored" "$(grep '^tab_folder=' "$CONF")" "tab_folder=file:///"
gt_key Escape; sleep 0.5
gt_key ctrl+t
gt_check "and New Tab opens it" "$(title)" "File System"
gt_end
