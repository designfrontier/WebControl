#!/usr/bin/env bash
# Flashes a test firmware onto the controller the way the holey upgrade does: flash-hex.sh <file.hex>
set -euo pipefail

SETTINGS="${WC_HOME:-$HOME}/.WebControl/webcontrol.json"
read -r -a SYSTEMCTL <<< "${WC_SYSTEMCTL:-sudo systemctl}"
AVRDUDE="${WC_AVRDUDE:-avrdude}"
# wc-firmware's BOOTLOADER_START: the application must end below the bootloader.
FLASH_LIMIT=$((0x3E000))

usage() {
  cat >&2 << 'EOF'
usage: flash-hex.sh <file.hex>

Stops webcontrol-rs, flashes <file.hex> to the controller on the COMport setting with the same avrdude command
line as Actions > Upgrade Holey Firmware, then starts webcontrol-rs again.

To roll back, use the UI's existing Actions > Upgrade Holey Firmware (51.292).
EOF
  exit 2
}

fail() {
  echo "flash-hex: $*" >&2
  exit 1
}

[ $# = 1 ] && [ -f "$1" ] || usage
hex="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"

check_hex() {
  python3 - "$1" "$FLASH_LIMIT" << 'EOF'
import sys
path, limit = sys.argv[1], int(sys.argv[2])
base, top, ended = 0, 0, False
for number, line in enumerate(open(path, encoding="ascii", errors="replace"), 1):
    line = line.strip()
    if not line:
        continue
    try:
        record = bytes.fromhex(line[1:]) if line[0] == ":" and not ended else b""
    except ValueError:
        record = b""
    if len(record) < 5 or len(record) != record[0] + 5 or sum(record) & 0xFF:
        sys.exit(f"line {number} is not an Intel HEX record")
    kind, data = record[3], record[4:-1]
    if kind == 0:
        top = max(top, base + (record[1] << 8 | record[2]) + len(data))
    elif kind == 1:
        ended = True
    elif kind in (2, 4):
        base = int.from_bytes(data, "big") << (4 if kind == 2 else 16)
    elif kind not in (3, 5):
        sys.exit(f"line {number} has an unknown record type {kind}")
if not ended or top == 0:
    sys.exit("no data, or no end-of-file record")
if top > limit:
    sys.exit(f"it ends at byte {top}, past the ATmega2560's {limit} below the bootloader")
EOF
}

comport() {
  python3 - "$SETTINGS" << 'EOF'
import json, sys
options = json.load(open(sys.argv[1])).get("Maslow Settings", [])
print(next((o.get("value", "") for o in options if o.get("key") == "COMport"), ""))
EOF
}

# wc-firmware's is_serial_port: nothing avrdude could read as an option or a path.
serial_port() {
  [[ "$1" != *..* ]] && [[ "$1" =~ ^/dev/(tty|cu)[A-Za-z0-9._-]+$ || "$1" =~ ^/dev/serial/by-(id|path)/[A-Za-z0-9._:+-]+$ ]]
}

reason="$(check_hex "$hex" 2>&1)" || fail "$1 refused: $reason"
[ -f "$SETTINGS" ] || fail "no $SETTINGS"
port="$(comport)" || fail "could not read COMport from $SETTINGS"
serial_port "$port" || fail "COMport '$port' is not a serial port"

echo "Stopping webcontrol-rs"
"${SYSTEMCTL[@]}" stop webcontrol-rs
trap 'echo "Starting webcontrol-rs"; "${SYSTEMCTL[@]}" start webcontrol-rs' EXIT
"$AVRDUDE" -patmega2560 -cwiring "-P$port" -b115200 -D "-Uflash:w:$hex:i"
echo "Flashed $1 to $port"
