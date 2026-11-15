#!/usr/bin/env bash
# Runs on the Pi: installs webcontrol-rs, enables it and starts it unless it is already running.
#   bash install-pi.sh <dir with webcontrol-rs, the unit, the drop-in, wc-switch and the sudoers rule>
# A running webcontrol.service (an older WebControl on the same disk) keeps the port; switch with `wc-switch rust`.
set -euo pipefail

src="${1:?usage: install-pi.sh <staging dir>}"
root="${WC_ROOT:-}"
read -r -a SYSTEMCTL <<< "${WC_SYSTEMCTL:-sudo systemctl}"
read -r -a SUDO <<< "${WC_SUDO:-sudo}"
read -r -a APT <<< "${WC_APT:-sudo apt-get install -y avrdude}"
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
# The firmware upgrade reads and writes the controller's flash with avrdude.
command -v avrdude > /dev/null || "${APT[@]}"

echo "Installed $("$opt/webcontrol-rs" --version 2> /dev/null || echo webcontrol-rs) for $user."
if "${SYSTEMCTL[@]}" is-active --quiet webcontrol; then
  echo "webcontrol.service, an older WebControl, is running and keeps port 5000. When it is idle: wc-switch rust"
  exit 0
fi
# A stopped but enabled older WebControl would race webcontrol-rs for the port at the next boot.
if "${SYSTEMCTL[@]}" is-enabled --quiet webcontrol 2> /dev/null; then "${SYSTEMCTL[@]}" disable webcontrol; fi
if "${SYSTEMCTL[@]}" is-active --quiet webcontrol-rs; then
  "${SYSTEMCTL[@]}" enable webcontrol-rs
  echo "webcontrol-rs is still running the previous binary, so a job carries on. When the machine is idle:"
  echo "  sudo systemctl restart webcontrol-rs"
else
  "${SYSTEMCTL[@]}" enable --now webcontrol-rs
  echo "webcontrol-rs is running and starts at boot: http://$(hostname).local:5000/"
fi
