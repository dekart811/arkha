#!/bin/bash
# exec-ask.sh x11|wayland: opening an executable script asks whether to
# run it, and Enter in that question doesn't run it (on FAT-formatted
# drives every file is executable); the Run button does.
GT_A11Y=1
. "$(dirname "$0")/lib.sh"
gt_session "$@"

mkdir -p "$HOME/drive"
# shellcheck disable=SC2016  # a script's text: $HOME is expanded when it runs
printf '#!/bin/sh\ntouch "$HOME/ran"\n' > "$HOME/drive/notes.txt"
chmod +x "$HOME/drive/notes.txt"
gt_display
gt_arkha "$HOME/drive"
asked() { gt_a11y dialogs | grep -c . ; }
gt_click 600 400; gt_type notes; gt_key Escape; gt_key Return; sleep 1
gt_shot asked
gt_check "opening it asks" "$(asked)" "1"
gt_key Return; sleep 1
gt_check "Enter doesn't run it" "$([ -e "$HOME/ran" ] && echo ran || echo not run) $(asked)" "not run 0"
gt_key Return; sleep 1
gt_ok "Run pressed" gt_a11y press Run; sleep 1
gt_check "Run runs it" "$([ -e "$HOME/ran" ] && echo ran || echo not run)" "ran"
gt_end
