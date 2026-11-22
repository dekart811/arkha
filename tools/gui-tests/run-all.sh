#!/bin/bash
# run-all.sh [x11|wayland ...] [-- TEST ...]: every GUI test (or the ones
# named) on X11 and Wayland (or the backends named); a line per test, the
# details of failures, and status 1 if any failed. Takes 25–30 minutes
# for each backend.
# Build first: cargo build --release.
D=$(cd "$(dirname "$0")" && pwd)
backends=(); tests=()
while [ $# -gt 0 ]; do
  case "$1" in --) shift; tests=("$@"); break ;; *) backends+=("$1"); shift ;; esac
done
[ ${#backends[@]} -eq 0 ] && backends=(x11 wayland)
if [ ${#tests[@]} -eq 0 ]; then
  for f in "$D"/*.sh; do case "$(basename "$f")" in lib.sh|run-all.sh|fixtures.sh) ;; *) tests+=("$(basename "$f" .sh)") ;; esac; done
fi
failed=0
for b in "${backends[@]}"; do
  for t in "${tests[@]}"; do
    start=$(date +%s)
    if out=$("$D/$t.sh" "$b" 2>&1); then
      printf 'ok    %-16s %-8s %3d s\n' "$t" "$b" $(( $(date +%s) - start ))
    else
      failed=1
      printf 'FAIL  %-16s %-8s %3d s\n' "$t" "$b" $(( $(date +%s) - start ))
      # shellcheck disable=SC2001  # each line indented
      echo "$out" | sed 's/^/      /'
    fi
  done
done
exit $failed
