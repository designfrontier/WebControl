#!/usr/bin/env bash
# Installs WebControl from this source tree on Raspberry Pi OS as the systemd service webcontrol.service.
# See spec/INSTALL-PI.md. Run it as the user WebControl should run as; it calls sudo itself.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: tools/install_from_source.sh [--activate | --rollback]

  (no option)  Create .venv (Python 3.9), install the requirements, check frontend/dist and install
               webcontrol.service. Never stops or restarts a WebControl that is already running.
  --activate   Stop the old WebControl service (v0.94), keeping it as webcontrol-v094.service for
               rollback, then enable and (re)start this one. Run it only with the machine idle.
  --rollback   Stop this service, put back the settings v0.94 last saved, and start webcontrol-v094.
EOF
}

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
venv="$repo/.venv"
staged="$repo/build/webcontrol.service"
unit=/etc/systemd/system/webcontrol.service
old_unit=/etc/systemd/system/webcontrol-v094.service
marker="# Installed by WebControl tools/install_from_source.sh"
port=5000
settings="$HOME/.WebControl/webcontrol.json"
settings_v094="$settings.v094"
piwheels=https://www.piwheels.org/simple
os_release=/etc/os-release

say() { printf '\n==> %s\n' "$*"; }
die() { printf '\nERROR: %s\n' "$*" >&2; exit 1; }

is_ours() { [ -f "$1" ] && grep -qF "$marker" "$1"; }
# Where systemd loads webcontrol.service from, if that is not this script's unit: /etc, /lib or elsewhere.
v094_unit() {
  local path
  path="$(systemctl show -p FragmentPath --value webcontrol.service 2>/dev/null || true)"
  [ -z "$path" ] || [ ! -f "$path" ] || is_ours "$path" || echo "$path"
}
unit_user() { local user; user="$(sed -n 's/^User=//p' "$1" | tail -n 1)"; echo "${user:-root}"; }
port_busy() { ss -ltnH "sport = :$port" 2>/dev/null | grep -q .; }
find_uv() { command -v uv || { [ -x "$HOME/.local/bin/uv" ] && echo "$HOME/.local/bin/uv"; } || true; }
# dpkg reports the userland, which is 32-bit on some Pis whose kernel uname calls aarch64.
deb_arch() { dpkg --print-architecture 2>/dev/null || uname -m; }
is_pi32() { case "$(deb_arch)" in armhf | armel | armv7l | armv6l) return 0 ;; *) return 1 ;; esac; }
# shellcheck source=/dev/null
os_codename() { (. "$os_release" 2>/dev/null && echo "${VERSION_CODENAME:-unknown}") || echo unknown; }
is_py39() { "$1" -c 'import sys; sys.exit(sys.version_info[:2] != (3, 9))' 2>/dev/null; }

explain_python() {
  die "Python 3.9 was not found, and WebControl's pinned requirements need it.
  Raspberry Pi OS 11 (Bullseye) ships Python 3.9 as python3. Buster has 3.7 and Bookworm 3.11.
  On those, install uv, open a new shell, and re-run this script; uv downloads its own Python 3.9:
    curl -LsSf https://astral.sh/uv/install.sh | sh"
}

report_old_webcontrol() {
  say "Looking for an existing WebControl (v0.94)"
  local found=0 v094
  v094="$(v094_unit)"
  if [ -n "$v094" ]; then
    echo "systemd unit $v094 (the webcontrol-pi image's unit):"
    grep -E '^(ExecStart|WorkingDirectory|User)=' "$v094" | sed 's/^/    /'
    found=1
  fi
  systemctl list-unit-files --type=service --no-legend 2>/dev/null | awk '{print "    service: " $1 " (" $2 ")"}' |
    grep -i webcontrol && found=1
  if port_busy; then
    echo "port $port is in use by:"
    ss -ltnpH "sport = :$port" | sed 's/^/    /'
    found=1
  fi
  local where
  for where in /etc/rc.local "$HOME/.config/autostart" /etc/xdg/autostart "$HOME/.config/lxsession"; do
    [ -e "$where" ] && grep -rIin webcontrol "$where" 2>/dev/null | sed 's/^/    mentioned in: /' && found=1
  done
  crontab -l 2>/dev/null | grep -i webcontrol | sed 's/^/    crontab: /' && found=1
  [ "$found" = 1 ] || echo "none found"
}

# The service runs as whoever runs this script, so v0.94's user must match or the settings would not carry over.
check_same_user() {
  local v094 owner
  v094="$(v094_unit)"
  [ -n "$v094" ] || { [ -f "$old_unit" ] && v094="$old_unit"; } || return 0
  owner="$(unit_user "$v094")"
  [ "$owner" = "$(id -un)" ] || die "v0.94 runs as $owner ($v094), so its settings are in ~$owner/.WebControl.
  Run this script as $owner, so the new service runs as $owner and uses the same settings."
}

check_dist() {
  [ -f "$repo/frontend/dist/index.html" ] || die "frontend/dist is missing. Build it on the Mac (cd frontend && npm ci &&
  npm run build) and copy it here with rsync, as spec/INSTALL-PI.md step 2 shows. Building Node on a Pi 3B is too slow."
}

# piwheels builds its Python 3.9 wheels on Bullseye only, against Bullseye's libraries.
check_platform() {
  is_pi32 || return 0
  [ "$(os_codename)" != bullseye ] || return 0
  die "32-bit Raspberry Pi OS $(os_codename) cannot run this install. Its Python 3.9 wheels come from piwheels,
  which builds them on Bullseye only, so OpenCV, NumPy and SciPy would not load here. Nothing was changed.
  Keep v0.94, or install 32-bit Raspberry Pi OS 11 (Bullseye) or 64-bit Raspberry Pi OS on a new SD card."
}

apt_packages() {
  local packages=() package
  [ -n "$(find_uv)" ] || packages+=(python3-venv)
  if is_pi32; then
    while read -r package; do packages+=("$package"); done < <(grep -E '^[a-z0-9]' "$repo/requirements-pi-apt.txt")
  else
    # PyPI's opencv-python wheel loads libGL and GLib, which Raspberry Pi OS Lite leaves out.
    packages+=(libgl1 libglib2.0-0)
  fi
  case "$(deb_arch)" in arm64 | aarch64) packages+=(gcc python3-dev) ;; esac
  say "Installing system packages: ${packages[*]}"
  sudo apt-get update
  sudo apt-get install -y --no-install-recommends "${packages[@]}"
}

venv_ok() {
  [ -x "$venv/bin/python" ] && is_py39 "$venv/bin/python" &&
    [ "$("$venv/bin/python" -c 'import platform; print(platform.system())')" = Linux ]
}

create_venv() {
  venv_ok && return 0
  if [ -e "$venv" ]; then
    say "Replacing .venv (wrong Python, or copied from another machine)"
    rm -rf "$venv"
  fi
  local uv candidate
  uv="$(find_uv)"
  if [ -n "$uv" ]; then
    say "Creating .venv with uv (Python 3.9)"
    "$uv" venv --python 3.9 "$venv" || explain_python
    return 0
  fi
  for candidate in python3.9 python3; do
    if command -v "$candidate" >/dev/null && is_py39 "$candidate"; then
      say "Creating .venv with $candidate -m venv"
      "$candidate" -m venv "$venv"
      return 0
    fi
  done
  explain_python
}

# piwheels lacks some pinned versions for armv7, so those pins are swapped for requirements-pi-overrides.txt.
effective_requirements() {
  local out="$repo/build/requirements-pi.txt" names
  mkdir -p "$repo/build"
  if is_pi32; then
    names="$(grep -E '^[A-Za-z]' "$repo/requirements-pi-overrides.txt" | sed 's/[=<>~!].*//' | paste -sd'|' -)"
    { grep -vE "^($names)==" "$repo/requirements.txt"; grep -E '^[A-Za-z]' "$repo/requirements-pi-overrides.txt"; } >"$out"
  else
    cp "$repo/requirements.txt" "$out"
  fi
  echo "$out"
}

install_requirements() {
  local requirements uv pip_args uv_args
  requirements="$(effective_requirements)"
  pip_args=(-r "$requirements")
  uv_args=(-r "$requirements")
  if is_pi32; then
    pip_args+=(--extra-index-url "$piwheels")
    uv_args+=(--extra-index-url "$piwheels" --index-strategy unsafe-best-match)
  fi
  uv="$(find_uv)"
  say "Installing Python requirements (a few minutes on a Pi 3B)"
  if [ -n "$uv" ]; then
    "$uv" pip install --python "$venv/bin/python" "${uv_args[@]}"
  else
    "$venv/bin/python" -m pip install --upgrade pip
    "$venv/bin/python" -m pip install "${pip_args[@]}"
  fi
  if ! "$venv/bin/python" -c 'import cv2, gevent, numpy, scipy.optimize, serial, flask_socketio' 2>"$repo/build/import-error.txt"; then
    cat "$repo/build/import-error.txt" >&2
    find "$venv/lib" -path '*site-packages/cv2/*.so' -exec ldd {} \; 2>/dev/null | grep 'not found' | sort -u | sed 's/^/    missing: /' >&2 || true
    die "the requirements installed but do not import. Install the system libraries listed as missing
  (apt-cache search <name> finds the package) and re-run this script."
  fi
}

write_unit() {
  local user
  user="$(id -un)"
  mkdir -p "$(dirname "$staged")"
  cat >"$staged" <<EOF
$marker
[Unit]
Description=WebControl (source install, classic UI at / and new UI at /next/)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=$user
WorkingDirectory=$repo
# The port is webPort in ~$user/.WebControl/webcontrol.json, $port by default.
ExecStart=$venv/bin/python $repo/main.py
Environment=PYTHONUNBUFFERED=1
# main.py opens a browser tab on start; a console browser would block a headless service.
Environment=BROWSER=true
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
}

print_switch_help() {
  cat <<EOF

The old WebControl keeps running. When the machine is idle, switch with:
    tools/install_from_source.sh --activate
EOF
  [ ! -f "$old_unit" ] || echo "Roll back with: tools/install_from_source.sh --rollback"
}

install_unit() {
  local v094
  write_unit
  v094="$(v094_unit)"
  if [ -n "$v094" ]; then
    if [ ! -f "$old_unit" ]; then
      say "Keeping a copy of the v0.94 unit as $old_unit (disabled, for rollback)"
      sudo cp "$v094" "$old_unit"
      sudo systemctl daemon-reload
    fi
    say "Staged the new unit at $staged; webcontrol.service still belongs to v0.94"
    print_switch_help
    return 0
  fi
  say "Installing $unit"
  sudo install -m 644 "$staged" "$unit"
  sudo systemctl daemon-reload
  if systemctl is-active --quiet webcontrol; then
    echo "webcontrol.service keeps running the previous code, so a cut in progress carries on."
    echo "When the machine is idle, restart it on the new code with: tools/install_from_source.sh --activate"
  elif port_busy; then
    echo "Port $port is in use (see above), so webcontrol.service was installed but not enabled or started."
    print_switch_help
  else
    sudo systemctl enable --now webcontrol
    wait_for_server
  fi
}

wait_for_server() {
  local path="${1:-/next/}" attempt
  say "Waiting for http://localhost:$port$path"
  for attempt in $(seq 1 120); do
    if curl -fsS -o /dev/null "http://localhost:$port$path"; then
      echo "WebControl is up after ${attempt}s: http://$(hostname -I | awk '{print $1}'):$port/ and /next/"
      return 0
    fi
    sleep 1
  done
  die "WebControl did not answer within 120s. Check: journalctl -u webcontrol -n 100"
}

# v0.94 reads every entry in webcontrol.json, so keys a newer backend adds could reach its firmware sync.
save_v094_settings() {
  [ -f "$settings" ] || return 0
  cp -p "$settings" "$settings_v094"
  echo "Saved v0.94's settings as $settings_v094 for --rollback"
}

activate() {
  [ -f "$staged" ] || die "run tools/install_from_source.sh first (no staged unit at $staged)"
  local v094 from_v094=0
  v094="$(v094_unit)"
  [ -z "$v094" ] || from_v094=1
  ! systemctl is-active --quiet webcontrol-v094 || from_v094=1
  if [ "$from_v094" = 1 ] || { [ -f "$old_unit" ] && [ ! -f "$settings_v094" ]; }; then
    save_v094_settings
  fi
  if [ -n "$v094" ]; then
    [ -f "$old_unit" ] || sudo cp "$v094" "$old_unit"
    say "Stopping and disabling the v0.94 webcontrol.service"
    sudo systemctl disable --now webcontrol
  fi
  if systemctl is-active --quiet webcontrol-v094 || systemctl is-enabled --quiet webcontrol-v094; then
    sudo systemctl disable --now webcontrol-v094
  fi
  sudo install -m 644 "$staged" "$unit"
  sudo systemctl daemon-reload
  if port_busy && ! systemctl is-active --quiet webcontrol; then
    report_old_webcontrol
    die "port $port is still in use by something other than a systemd unit. Stop it (see above) and re-run --activate."
  fi
  say "Starting the new webcontrol.service"
  sudo systemctl enable webcontrol
  sudo systemctl restart webcontrol
  wait_for_server
}

rollback() {
  [ -f "$old_unit" ] || die "there is no $old_unit to roll back to"
  say "Stopping the new webcontrol.service"
  sudo systemctl disable --now webcontrol
  if [ -f "$settings_v094" ]; then
    [ ! -f "$settings" ] || cp -p "$settings" "$settings.next"
    cp -p "$settings_v094" "$settings"
    echo "Put back v0.94's settings; the new build's last settings are in $settings.next"
  fi
  say "Starting webcontrol-v094.service"
  sudo systemctl enable --now webcontrol-v094
  wait_for_server /
}

main() {
  local mode=install
  case "${1:-}" in
    "") ;;
    --activate) mode=activate ;;
    --rollback) mode=rollback ;;
    -h | --help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
  [ "$(id -u)" != 0 ] || die "run this as your normal user (for example pi), not with sudo; it calls sudo when it needs to"
  [ "$(uname -s)" = Linux ] || die "this installs a systemd service on Raspberry Pi OS; on macOS use the README's dev setup"
  cd "$repo"
  check_same_user
  if [ "$mode" != install ]; then
    "$mode"
    return
  fi
  check_dist
  check_platform
  report_old_webcontrol
  id -nG | tr ' ' '\n' | grep -qx dialout || echo "WARNING: $(id -un) is not in the dialout group, so /dev/ttyACM0 may be unreadable: sudo usermod -aG dialout $(id -un)"
  apt_packages
  create_venv
  install_requirements
  install_unit
}

# Sourcing the script (as the tests do) defines the functions without installing anything.
[ "${BASH_SOURCE[0]}" != "$0" ] || main "$@"
