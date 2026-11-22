#!/bin/bash
# checksums.sh x11|wayland: the Checksums tab of Properties. Nothing is
# worked out until asked; SHA-256 of a 300 MB file matches sha256sum's, the
# other Calculate buttons waiting while it runs; Cancel stops one; Copy puts
# it on the clipboard; Calculate All works out the rest in one read (each
# matching its sum program). Only the checksums chosen are offered (MD5
# turned off here), in their order.
GT_A11Y=1 GT_CONF='checksum_order=sha256;crc32;md5;sha1;sha512;sha384;blake2b;
hidden_checksums=md5;sha384;blake2b;'
. "$(dirname "$0")/lib.sh"
gt_session "$@"

head -c $((300 << 20)) /dev/urandom > "$HOME/big.bin"
want=$(sha256sum "$HOME/big.bin" | cut -d' ' -f1)
gt_display
gt_arkha "$HOME"
gt_click 900 650; gt_type big; gt_key Escape
gdbus call --session --dest $APP_ID --object-path $APP_PATH/window/1 \
  --method org.gtk.Actions.Activate properties [] {} >/dev/null & sleep 1.5
gt_ok "the Checksums tab" gt_a11y tab Checksums; sleep 0.5; gt_shot tab
# The dialog's rows, top to bottom: ROLE NAME... (by where they are on screen).
rows() { python3 - "$@" <<'PY'
import sys, gi; gi.require_version('Atspi', '2.0'); from gi.repository import Atspi
Atspi.init(); d = Atspi.get_desktop(0)
app = [d.get_child_at_index(i) for i in range(d.get_child_count()) if d.get_child_at_index(i).get_name() == 'arkha'][0]
dlg = [app.get_child_at_index(i) for i in range(app.get_child_count()) if app.get_child_at_index(i).get_role_name() == 'dialog'][0]
stack, found = [dlg], []
while stack:
    x = stack.pop()
    if x.get_role_name() == sys.argv[1] and x.get_name() in sys.argv[2:] and x.get_state_set().contains(Atspi.StateType.SHOWING):
        found.append((Atspi.Component.get_extents(x, Atspi.CoordType.SCREEN).y, x))
    stack.extend(c for c in (x.get_child_at_index(k) for k in range(x.get_child_count() - 1, -1, -1)) if c)
found.sort(key=lambda f: f[0])
if len(sys.argv) > 3 and sys.argv[2] == 'Calculate':
    found[int(sys.argv[3])][1].get_action_iface().do_action(0)
else:
    print(' '.join(x.get_name() for _, x in found))
PY
}
gt_check "the chosen checksums, in their order" "$(rows label SHA-256 CRC32 MD5 SHA-1 SHA-512 SHA-384 BLAKE2b)" "SHA-256 CRC32 SHA-1 SHA-512"
gt_check "none worked out yet" "$(gt_a11y labels | grep -cE '^[0-9a-f]{8,}$')" "0"
calc() { rows button Calculate "$1"; }
calc 0; sleep 0.4; gt_shot running
gt_check "while SHA-256 runs the other buttons wait" "$(gt_a11y buttons | grep -cE '^Calculate( All)?\|off$')" "5"
for _ in $(seq 60); do gt_a11y labels | grep -qx "$want" && break; sleep 0.5; done
gt_shot "done"
gt_check "SHA-256 matches sha256sum" "$(gt_a11y labels | grep -x "$want")" "$want"
gt_check "and the buttons are back" "$(gt_a11y buttons | grep -cE '^Calculate( All)?\|on$')" "5"
calc 3; sleep 0.3
gt_ok "Cancel" gt_a11y press Cancel; sleep 0.8; gt_shot cancelled
gt_check "cancelled: no SHA-512, the buttons back" "$(gt_a11y labels | grep -cE '^[0-9a-f]{128}$') $(gt_a11y buttons | grep -cE '^Calculate( All)?\|on$')" "0 5"
gt_ok "Copy SHA-256" gt_a11y press "Copy SHA-256"; sleep 0.3
# Calculate All: the three left, in one go.
gt_ok "Calculate All" gt_a11y press "Calculate All"; sleep 0.4
gt_check "meanwhile every button waits" "$(gt_a11y buttons | grep -cE '^Calculate( All)?\|off$')" "5"
for _ in $(seq 90); do [ "$(gt_a11y labels | grep -cE '^[0-9a-f]{8,}$')" = 4 ] && break; sleep 0.5; done
gt_shot all
sums=$(gt_a11y labels | grep -xE '[0-9a-f]{8,}' | sort | tr '\n' ' ')
crc=$(python3 -c "import zlib,sys; print('%08x' % (zlib.crc32(open(sys.argv[1],'rb').read()) & 0xffffffff))" "$HOME/big.bin")
gt_check "Calculate All: CRC32, SHA-1 and SHA-512 match crc32/sha1sum/sha512sum" "$sums" "$(printf '%s\n' "$want" "$crc" "$(sha1sum "$HOME/big.bin" | cut -d' ' -f1)" "$(sha512sum "$HOME/big.bin" | cut -d' ' -f1)" | sort | tr '\n' ' ')"
gt_key Escape; sleep 0.5
gt_key ctrl+l; gt_key ctrl+a; gt_key ctrl+v; sleep 0.3
gt_check "Copy put it on the clipboard" "$(gt_a11y focused-text)" "$want"
gt_key Escape
gt_end
