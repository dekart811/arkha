#!/bin/bash
# connect.sh x11|wayland: File → Connect to Server. The fields shown follow
# the type (a Windows share has a share and a domain, SSH hasn't); Connect
# with "Add bookmark" bookmarks the address under its name and opens it,
# which mounts it: here a server that refuses (127.0.0.1, port 1), so the
# window says it couldn't open it. With ARKHA_TEST_SFTP=HOST (an SSH host
# reached without a password, e.g. one of ~/.ssh/config's), sftp://HOST/ is
# also opened from the location bar: mounted, its files listed, and the
# path bar's first button named after the mount, not "File System". With
# ARKHA_TEST_SMB=USER@HOST/SHARE (a share that wants a password; needs
# gvfs-smb), opening it brings up GTK's password dialog, and cancelling it
# leaves no error behind.
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

gt_display
gt_arkha "$HOME"
labels() { gt_a11y labels | tr '\n' '|'; }
has() { case "|$1" in *"|$2|"*) echo yes ;; *) echo "no: $1" ;; esac; }

gt_act connect-server; sleep 1
gt_check "the dialog" "$(gt_a11y dialogs | tr '\n' '|')" "Connect to Server|"
l=$(labels)
gt_check "SSH: a user name" "$(has "$l" "User name:")" "yes"
gt_check "SSH: no share" "$(has "$l" "Share:")" "no: $l"
# The type: Windows share (the fourth).
gt_key alt+t; gt_key Down Down Down; sleep 0.5
l=$(labels)
gt_check "Windows share: a share" "$(has "$l" "Share:")" "yes"
gt_check "Windows share: a domain" "$(has "$l" "Domain name:")" "yes"
gt_key Up Up Up
gt_key alt+s; gt_type "127.0.0.1"
gt_key alt+p; gt_type "1"
gt_key alt+f; gt_type "/srv"
gt_key alt+b; gt_key alt+n; gt_type "Nowhere"
gt_key Return; sleep 3; gt_quiet
gt_check "bookmarked under its name" "$(cat "$XDG_CONFIG_HOME/gtk-3.0/bookmarks" 2>/dev/null)" "sftp://127.0.0.1:1/srv Nowhere"
w=$(gt_a11y window-labels | tr '\n' '|')
# shellcheck disable=SC1111  # Arkha's own quotes, as in its message
case "$w" in *"Could not open “sftp://127.0.0.1:1/srv”"*) s=yes ;; *) s="no: $w" ;; esac
gt_check "a server that refuses: the window says so" "$s" "yes"

if [ -n "$ARKHA_TEST_SFTP" ]; then
  gt_go "sftp://$ARKHA_TEST_SFTP/" 4
  w=$(gt_a11y window-labels | tr '\n' '|')
  case "$w" in *"|$ARKHA_TEST_SFTP|"*) s=yes ;; *) s="no: $w" ;; esac
  gt_check "sftp://$ARKHA_TEST_SFTP/ mounted (in the side pane)" "$s" "yes"
  case "$w" in *" items, "*) s=yes ;; *) s="no: $w" ;; esac
  gt_check "and its files listed" "$s" "yes"
  gt_ok "the path bar's first button named after the mount" gt_a11y where "$ARKHA_TEST_SFTP"
fi

if [ -n "$ARKHA_TEST_SMB" ]; then
  gt_go "smb://$ARKHA_TEST_SMB" 3
  l=$(labels)
  gt_check "smb://: GTK asks for the password" "$(has "$l" "Authentication Required")" "yes"
  gt_key Escape; sleep 2; gt_quiet
  gt_check "cancelled: the dialog closed" "$(gt_a11y dialogs | wc -l)" "0"
  w=$(gt_a11y window-labels | tr '\n' '|')
  case "$w" in *"Could not open"*) s="no: $w" ;; *) s=yes ;; esac
  gt_check "cancelled: no error shown" "$s" "yes"
fi
gt_end
