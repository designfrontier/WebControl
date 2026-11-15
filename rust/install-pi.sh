#!/usr/bin/env bash
# Runs on the Pi: installs webcontrol-rs beside the Python backend without starting it.
#   bash install-pi.sh <dir with webcontrol-rs, the unit, the drop-in, wc-switch and the sudoers rule>
# The Python webcontrol.service stays enabled and running; switch with `wc-switch rust`.
set -euo pipefail

src="${1:?usage: install-pi.sh <staging dir>}"
root="${WC_ROOT:-}"
read -r -a SYSTEMCTL <<< "${WC_SYSTEMCTL:-sudo systemctl}"
read -r -a SUDO <<< "${WC_SUDO:-sudo}"
user="${WC_USER:-${SUDO_USER:-$(id -un)}}"
opt="$root/opt/webcontrol-rs"
units="$root/etc/systemd/system"

if [ -z "$root" ]; then
  [ "$(uname -m)" = aarch64 ] || { echo "webcontrol-rs is built for 64-bit ARM; this is $(uname -m)" >&2; exit 1; }
  "$src/webcontrol-rs" --version > /dev/null || { echo "webcontrol-rs does not run here (glibc 2.36+ needed)" >&2; exit 1; }
fi

"${SUDO[@]}" install -d "$opt" "$units/webcontrol.service.d" "$root/usr/local/bin" "$root/etc/sudoers.d"
if [ -f "$opt/webcontrol-rs" ]; then "${SUDO[@]}" mv "$opt/webcontrol-rs" "$opt/webcontrol-rs.prev"; fi
"${SUDO[@]}" install -m 755 "$src/webcontrol-rs" "$opt/webcontrol-rs"
sed "s/^User=.*/User=$user/" "$src/webcontrol-rs.service" | "${SUDO[@]}" tee "$units/webcontrol-rs.service" > /dev/null
"${SUDO[@]}" install -m 644 "$src/webcontrol-conflicts.conf" "$units/webcontrol.service.d/conflicts.conf"
"${SUDO[@]}" install -m 755 "$src/wc-switch" "$root/usr/local/bin/wc-switch"
# Power-off from the UI; a sudoers file that fails visudo would lock sudo out, so it is checked first.
rule="$(mktemp)"
sed "s/^pi /$user /" "$src/webcontrol-rs.sudoers" > "$rule"
if [ -z "$root" ]; then visudo -cqf "$rule" || { echo "the sudoers rule does not parse" >&2; exit 1; }; fi
"${SUDO[@]}" install -m 440 "$rule" "$root/etc/sudoers.d/webcontrol-rs"
rm -f "$rule"
"${SYSTEMCTL[@]}" daemon-reload

echo "Installed $("$opt/webcontrol-rs" --version 2> /dev/null || echo webcontrol-rs) for $user."
echo "The Python backend is untouched. Try Rust with: wc-switch rust   (back with: wc-switch python)"
