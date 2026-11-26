#!/bin/bash
# version.sh x11|wayland: arkha -v and --version print the version as About
# shows it and quit, before any window (run without a display here, so
# opening one would fail) and also with a folder given; --help lists them,
# in English and in German (language=de in the settings). With Arkha
# running, -v doesn't make it open a window.
. "$(dirname "$0")/lib.sh"
gt_session "$@"

run() { env -u DISPLAY -u WAYLAND_DISPLAY timeout 10 "$ARKHA" "$@" 2>&1; echo "exit $?"; }
want=$(grep -m1 '^version' "$GT_ROOT/Cargo.toml" | cut -d'"' -f2)
for opt in --version -v; do
  out=$(run $opt)
  gt_check "$opt: the version, then quits" "$(echo "$out" | head -1 | cut -d' ' -f1,2) $(echo "$out" | tail -1)" "Arkha $want exit 0"
done
gt_check "-v with a folder: still only the version" "$(run -v "$HOME" | tr '\n' ' ')" "$(run -v | tr '\n' ' ')"
gt_check "--help lists it" "$(run --help | grep -E -- '-v, --version' | sed 's/  */ /g')" " -v, --version Show Arkha's version and quit"
gt_check "--help names the folders it takes" "$(run --help | grep -E '^ +arkha ' | sed 's/^ *//')" "arkha [OPTION…] [FOLDER…]"
# With Arkha running: answered all the same, no window opened by it.
gt_display
gt_arkha "$HOME"; sleep 1
out=$(timeout 10 "$ARKHA" -v 2>&1); sleep 1
gt_check "Arkha running: -v answers, no new window" "$out | $(gdbus introspect --session --dest $APP_ID --object-path $APP_PATH/window 2>/dev/null | grep -c 'node [0-9]')" "$(run -v | head -1) | 1"
gt_quit
printf '[General]\nlanguage=de\n' > "$XDG_CONFIG_HOME/arkha/arkha.conf"
gt_check "German: --help lists it" "$(run --help | grep -E -- '-v, --version' | sed 's/  */ /g')" " -v, --version Arkhas Version zeigen und beenden"
gt_end
