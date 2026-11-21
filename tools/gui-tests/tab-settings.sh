#!/bin/bash
# tab-settings.sh x11|wayland: Preferences → Behavior → Tabs → "New tabs
# open in:". New Tab (Ctrl+T) opens the current tab's folder (the
# default), home, the folder of the last closed tab, or a folder of one's
# own; a folder opened in a new tab (Ctrl+Enter) is that folder whatever
# the setting. The folder picked from the folder button's own list (File
# System) is the one stored (it once stayed the one before: only folders
# from its dialog were); a network place that can't be reached stays the
# one stored when Preferences opens (only a pick of one's own is stored,
# not what the button shows). The window's title is the current tab's
# folder.
GT_A11Y=1 GT_CONF='default_view=list'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/A/D" "$HOME/B" "$HOME/C"
CONF=$XDG_CONFIG_HOME/arkha/arkha.conf

gt_display
gt_arkha "$HOME/A"
gt_key ctrl+t
gt_check "New Tab: the current tab's folder (default)" "$(gt_title 0.8)" "A"
gt_quit

gt_conf tab_opens home
gt_arkha "$HOME/A"
gt_key ctrl+t
gt_check "New Tab: the home folder" "$(gt_title 0.8)" "Home"
# A folder opened in a new tab is that folder, not home.
gt_key ctrl+w; sleep 0.5
gt_click 300 147; sleep 0.3; gt_click 300 147; sleep 0.3
gt_key ctrl+Return; sleep 1
gt_check "a folder opened in a new tab is shown there" "$(gt_a11y tabs | grep -c '^D$')" "1"
gt_quit

gt_conf tab_opens folder; gt_conf tab_folder "file://$HOME/C"
gt_arkha "$HOME/A"
gt_key ctrl+t
gt_check "New Tab: the folder chosen" "$(gt_title 0.8)" "C"
# A tab in B, closed: B is the last closed tab's folder.
gt_go "$HOME/B"; gt_key ctrl+w
gt_quit

gt_conf tab_opens last
gt_arkha "$HOME/A"
gt_key ctrl+t
gt_check "New Tab: the folder of the last closed tab" "$(gt_title 0.8)" "B"

# The choice on Preferences' Behavior page.
gt_prefs 1
gt_shot behavior
gt_check "Preferences offers it" "$(gt_a11y labels | grep -c '^New tabs open in:$')" "1"
gt_quit

# A network place that can't be reached: Preferences opened and closed,
# it's still the one stored.
gt_conf tab_opens folder; gt_conf tab_folder "smb://nowhere.invalid/share"
gt_arkha "$HOME/A"
gt_prefs 1; sleep 1.5
gt_key Escape; sleep 0.5
gt_quit
gt_check "opening Preferences keeps a network place stored" "$(grep '^tab_folder=' "$CONF")" "tab_folder=smb://nowhere.invalid/share"

# "This folder:", File System picked from the button's list.
gt_conf tab_opens folder; gt_conf tab_folder "file://$HOME/C"
gt_arkha "$HOME/A"
gt_prefs 1
dy=0; [ "$BACKEND" = wayland ] && dy=14
gt_click 930 $((318 + dy)); sleep 0.8
gt_shot button-list
# The list opens at the folder shown (C, after home, Desktop, File
# System): one up is File System.
gt_key Up; gt_key Return; sleep 0.8
gt_check "File System from the button's list is stored" "$(grep '^tab_folder=' "$CONF")" "tab_folder=file:///"
gt_key Escape; sleep 0.5
gt_key ctrl+t
gt_check "and New Tab opens it" "$(gt_title 0.8)" "File System"
gt_end
