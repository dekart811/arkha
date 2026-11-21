#!/bin/bash
# props-tabs.sh x11|wayland: which tabs Properties shows, chosen in
# Preferences → Properties. Unticking Notes and Checksums there saves them
# as hidden, Basic can't be unticked, and a file's Properties then shows
# neither; ticking Notes again brings it back. A right-click on a tab
# (not Basic) hides it too. Through accessibility.
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

echo hello > "$HOME/a.txt"
gt_display
gt_arkha "$HOME"
hidden() { sed -n 's/^hidden_props_tabs=//p' "$XDG_CONFIG_HOME/arkha/arkha.conf"; }
tabs() { gt_a11y dialog-tabs | tr '\n' ' '; }
prefs() {
  # Properties: the tenth page.
  gt_prefs 9
}
props() {
  gt_click 900 650; gt_type a.t; gt_key Escape
  gdbus call --session --dest $APP_ID --object-path $APP_PATH/window/1 \
    --method org.gtk.Actions.Activate properties [] {} >/dev/null & sleep 1.5
}

prefs; gt_shot prefs-page
# (The tabs' names are their checkboxes', which press finds below.)
l=$(gt_a11y labels | tr '\n' '|')
case "$l" in *"|Tabs|"*"|Checksums|"*) s=yes ;; *) s="no: $l" ;; esac
gt_check "Preferences has the Properties page" "$s" "yes"
gt_ok "Notes unticked" gt_a11y press Notes
gt_ok "Checksums unticked" gt_a11y press Checksums
gt_a11y press Basic
gt_check "saved as hidden; Basic can't be" "$(hidden)" "notes;checksums;"
gt_key Escape; sleep 0.5

props; gt_shot props-without
gt_check "a file's Properties without Notes and Checksums" "$(tabs)" "Basic Emblems Permissions Open With "
gt_key Escape; sleep 0.5

prefs
gt_ok "Notes ticked again" gt_a11y press Notes
gt_check "only Checksums hidden now" "$(hidden)" "checksums;"
gt_key Escape; sleep 0.5
props
gt_check "Notes back in Properties" "$(tabs)" "Basic Emblems Permissions Open With Notes "
# A right-click on a tab: Hide This Tab (its key H). Not on Basic. On X11
# only: under Wayland, accessibility gives a dialog's tabs' places in the
# dialog, not on the screen, so a click there would miss them.
if [ "$BACKEND" = x11 ]; then
xy=$(gt_a11y where-tab Basic); gt_click $xy 3; sleep 0.5; gt_shot basic-right-click
gt_check "no menu on Basic" "$(gt_a11y windows | grep -c '^$')" "0"
xy=$(gt_a11y where-tab Permissions); gt_click $xy 3; sleep 0.5; gt_shot tab-menu
gt_key h; sleep 0.5; gt_quiet
gt_check "Permissions hidden at once" "$(tabs)" "Basic Emblems Open With Notes "
gt_check "and kept hidden" "$(hidden)" "checksums;permissions;"
gt_key Escape; sleep 0.5
props
gt_check "the next Properties without it too" "$(tabs)" "Basic Emblems Open With Notes "
fi
gt_end
