#!/bin/bash
# thumb-network.sh x11|wayland: thumbnails on a network place (Show
# thumbnails: Always) go with their files, as local ones do. Over SFTP to
# an sshd of the test's own (on 127.0.0.1, as the user running the test,
# serving the run folder): a video renamed keeps its thumbnail, as does one
# in a folder renamed; moved to a local folder, it keeps it there; deleted,
# its thumbnail goes. GVfs's ssh is given a key, config and known_hosts of
# the test's own (ssh reads the user's ~/.ssh by the account's home, not
# $HOME), so the user's are never read or written. Needs sshd, GVfs's sftp
# backend and FUSE (thumbnails are made from GVfs's path for the file).
# With ARKHA_TEST_MOUNT=DIR (a network share mounted there, e.g. by fstab:
# its files have file:// URIs) the same steps in a folder made in DIR,
# arkha-thumb-test-BACKEND, removed afterwards; no sshd then.
GT_A11Y=1 GT_CONF='show_thumbnails=always'
. "$(dirname "$0")/lib.sh"
if [ -n "$ARKHA_TEST_MOUNT" ]; then
  [ -d "$ARKHA_TEST_MOUNT" ] && [ -w "$ARKHA_TEST_MOUNT" ] || { echo "no folder to write in at $ARKHA_TEST_MOUNT" >&2; exit 2; }
else
  for need in /usr/bin/sshd /usr/lib/gvfsd-sftp /usr/lib/gvfsd-fuse; do
    [ -x "$need" ] || { echo "SKIP no $need"; exit 0; }
  done
fi
gt_session "$@"

mkdir -p "$HOME/local"
if [ -n "$ARKHA_TEST_MOUNT" ]; then
  R=$ARKHA_TEST_MOUNT/arkha-thumb-test-$BACKEND
  [ -e "$R" ] && { echo "FAIL $R is there already (left by an earlier run?)" >> "$OUT/result"; exit 0; }
  mkdir -p "$R/sub"
  trap 'rm -rf "$R"' EXIT
else
R=$HOME/remote; mkdir -p "$R/sub"
S=$OUT/sshd; mkdir -p "$S/bin"
PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
ssh-keygen -q -t ed25519 -N '' -f "$S/host"
ssh-keygen -q -t ed25519 -N '' -f "$S/client"
cp "$S/client.pub" "$S/auth"
echo "[127.0.0.1]:$PORT $(cut -d' ' -f1,2 "$S/host.pub")" > "$S/known_hosts"
cat > "$S/sshd_config" <<EOF
Port $PORT
ListenAddress 127.0.0.1
HostKey $S/host
PidFile $S/pid
AuthorizedKeysFile $S/auth
UsePAM no
StrictModes no
PasswordAuthentication no
KbdInteractiveAuthentication no
Subsystem sftp internal-sftp
EOF
cat > "$S/ssh_config" <<EOF
Host *
  IdentityFile $S/client
  IdentitiesOnly yes
  UserKnownHostsFile $S/known_hosts
  GlobalKnownHostsFile /dev/null
  StrictHostKeyChecking yes
EOF
cat > "$S/bin/ssh" <<EOF
#!/bin/sh
echo "\$@" >> "$S/ssh-calls"
unset SSH_AUTH_SOCK
exec /usr/bin/ssh -F "$S/ssh_config" "\$@"
EOF
chmod +x "$S/bin/ssh"
# GVfs (started by D-Bus on first use) gets the wrapper first in its PATH.
# It starts FUSE unless GVFS_DISABLE_FUSE is set at all (gt_session sets it
# only for the test's own programs): no path, no thumbnails.
dbus-update-activation-environment PATH="$S/bin:$PATH"
fi

ffmpeg -loglevel error -f lavfi -i testsrc=duration=2:size=320x240:rate=15 "$R/clip.mp4"
cp "$R/clip.mp4" "$R/other.mp4"
cp "$R/clip.mp4" "$R/sub/inner.mp4"
# A file's URI on the network place, and on the local disk.
luri() { python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).as_uri())' "$1"; }
if [ -n "$ARKHA_TEST_MOUNT" ]; then
  ruri() { luri "$1"; }
else
  ruri() { python3 -c 'import sys, urllib.parse; print("sftp://127.0.0.1:%s%s" % (sys.argv[1], urllib.parse.quote(sys.argv[2])))' "$PORT" "$1"; }
fi
# The URI in the thumbnail kept for URI $1 (its name: the URI's MD5).
thumb_of() {
  local t; t=$(find "$HOME/.cache/thumbnails" -name "$(printf %s "$1" | md5sum | cut -d' ' -f1).png" -not -path '*/fail/*' | head -1)
  [ -n "$t" ] && python3 - "$t" <<'PY'
import struct, sys
b = open(sys.argv[1], "rb").read()[8:]
while len(b) >= 12:
    n, = struct.unpack(">I", b[:4])
    kind, data = b[4:8], b[8:8 + n]
    if kind == b"tEXt" and data.startswith(b"Thumb::URI\0"):
        print(data[11:].decode())
    b = b[12 + n:]
PY
}
made() { grep -c 'thumbnail: running' "$OUT/arkha.log"; }
export G_MESSAGES_DEBUG=arkha
gt_display
if [ -z "$ARKHA_TEST_MOUNT" ]; then
  /usr/bin/sshd -D -e -f "$S/sshd_config" 2> "$S/sshd.log" & GT_PIDS="$GT_PIDS $!"
  sleep 0.5
fi
gt_arkha "$HOME"; sleep 1
gt_go "$(ruri "$R")" 4; sleep 2; gt_quiet
gt_shot mounted
if [ -z "$ARKHA_TEST_MOUNT" ]; then
  gt_check "mounted through the test's own ssh" "$([ -s "$S/ssh-calls" ] && echo yes || echo no)" "yes"
  gt_check "GVfs gives the files a path (FUSE)" "$(find "$XDG_RUNTIME_DIR/gvfs" -maxdepth 1 -name 'sftp*' 2>/dev/null | wc -l)" "1"
fi
gt_check "both videos get a thumbnail under their URI there" "$(thumb_of "$(ruri "$R/clip.mp4")") $(gt_thumbs) $(made)" "$(ruri "$R/clip.mp4") 2 2"
# Renamed.
gt_click 600 400; gt_type clip; gt_key Escape
gt_key F2; gt_key ctrl+a; gt_type renamed.mp4; gt_key Return; sleep 3; gt_quiet
gt_check "renamed: its thumbnail renamed, none made" "$([ -e "$R/renamed.mp4" ] && echo renamed) $(thumb_of "$(ruri "$R/renamed.mp4")") $(gt_thumbs) $(made)" "renamed $(ruri "$R/renamed.mp4") 2 2"
# A folder with a video in it, its thumbnail made, then the folder renamed.
gt_go "$(ruri "$R/sub")" 1; sleep 2; gt_quiet
gt_check "the video in the folder gets a thumbnail" "$(gt_thumbs) $(made)" "3 3"
gt_go "$(ruri "$R")" 1
gt_click 600 400; gt_type sub; gt_key Escape
gt_key F2; gt_key ctrl+a; gt_type folder; gt_key Return; sleep 3; gt_quiet
gt_check "folder renamed: the thumbnail in it renamed" "$([ -e "$R/folder/inner.mp4" ] && echo renamed) $(thumb_of "$(ruri "$R/folder/inner.mp4")") $(gt_thumbs)" "renamed $(ruri "$R/folder/inner.mp4") 3"
gt_go "$(ruri "$R/folder")" 1; sleep 2; gt_quiet
gt_check "shown there: none made" "$(made)" "3"
# Moved to a local folder (copied over, the original deleted).
gt_go "$(ruri "$R")" 1
gt_click 600 400; gt_type renamed; gt_key Escape; gt_key ctrl+x
gt_go "$HOME/local" 1; gt_key ctrl+v; sleep 3; gt_quiet
gt_shot moved
gt_check "moved to a local folder: the thumbnail with it, none made" "$([ -e "$HOME/local/renamed.mp4" ] && [ ! -e "$R/renamed.mp4" ] && echo moved) $(thumb_of "$(luri "$HOME/local/renamed.mp4")") $(gt_thumbs) $(made)" "moved $(luri "$HOME/local/renamed.mp4") 3 3"
# Deleted permanently on the network place (Enter confirms).
gt_go "$(ruri "$R")" 1
gt_click 600 400; gt_type other; gt_key Escape
gt_key shift+Delete; sleep 0.5; gt_key Return; sleep 2; gt_quiet
gt_check "deleted: its thumbnail gone" "$([ -e "$R/other.mp4" ] && echo there || echo deleted) $(thumb_of "$(ruri "$R/other.mp4")")$(gt_thumbs)" "deleted 2"
gt_end
