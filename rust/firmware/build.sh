#!/usr/bin/env bash
# Builds the holey 51.292 application hex from the Orob-Maslow/Firmware v51.292 tag and checks it against the
# pinned SHA-256. Usage: build.sh <out.hex>. Needs git, python3 and network; PlatformIO goes in a throwaway venv.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out="${1:?usage: build.sh <out.hex>}"
repo=https://github.com/Orob-Maslow/Firmware.git
commit=0064955de1bc0ea6a186e1e124bbdd5534890d17
platformio_version=6.1.16
atmelavr_version=4.2.0
servo_version=1.2.2
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

git init --quiet "$work/src"
git -C "$work/src" fetch --quiet --depth 1 "$repo" "$commit"
git -C "$work/src" checkout --quiet FETCH_HEAD
[ "$(git -C "$work/src" rev-parse HEAD)" = "$commit" ]

"${PYTHON:-python3}" -m venv "$work/pio"
"$work/pio/bin/pip" install --quiet "platformio==$platformio_version"
# The tag pins atmelavr@1.12.2, which the PlatformIO registry no longer serves; this pin fixes the compiler instead.
sed -i.orig 's/^platform = atmelavr@1\.12\.2$/platform = atmelavr@'"$atmelavr_version"'/' "$work/src/platformio.ini"
grep -q "^platform = atmelavr@$atmelavr_version\$" "$work/src/platformio.ini"
export PLATFORMIO_CORE_DIR="$work/core"
# Newer Arduino cores no longer bundle Servo, which Maslow.h includes.
(cd "$work/src" && "$work/pio/bin/pio" pkg install -e megaatmega2560 -l "arduino-libraries/Servo@$servo_version" >&2)
(cd "$work/src" && "$work/pio/bin/pio" run -e megaatmega2560 >&2)

hex="$work/src/.pio/build/megaatmega2560/firmware.hex"
actual="$( (sha256sum "$hex" 2>/dev/null || shasum -a 256 "$hex") | cut -d' ' -f1)"
pinned="$(tr -d '[:space:]' <"$here/holey-51.292.sha256")"
if [ "$actual" != "$pinned" ]; then
  echo "firmware.hex is $actual, but holey-51.292.sha256 pins $pinned" >&2
  [ -n "${KEEP_UNPINNED:-}" ] && cp "$hex" "$out"
  exit 1
fi
cp "$hex" "$out"
echo "$out: $actual"
