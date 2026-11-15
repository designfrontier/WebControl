#!/usr/bin/env bash
# Runs spec/INSTALL-PI.md's copy and install steps in a 64-bit Debian container with systemd, then checks the service.
# Usage: tools/test-install/run.sh [bookworm] [trixie]   (KEEP=1 leaves the containers running)
# Single-quoted commands run as pi inside the container, so they expand there.
# shellcheck disable=SC2016,SC2088
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
here="$repo/tools/test-install"
excludes=(--exclude .git --exclude .venv --exclude node_modules --exclude build --exclude releases --exclude .claude
  --exclude /firmware --exclude /dist)
failures=0

say() { printf '\n==> [%s] %s\n' "$distro" "$*"; }
fail() { printf 'FAIL [%s]: %s\n' "$distro" "$*" >&2; failures=$((failures + 1)); }
as_pi() { docker exec -u pi -w /home/pi "$name" bash -c "$1"; }
used_mb() { docker exec "$name" df -m --output=used / | tail -n 1; }
http_code() { as_pi "curl -s -o /dev/null -w '%{http_code}' http://localhost:5000$1" || true; }

# The owner's settings: the defaults with a few values changed, as an old Pi's ~/.WebControl would hold them.
fixture_settings() {
  jq '(.. | objects | select(.key == "motorSpacingX") | .value) = "3011.16237"
    | (.. | objects | select(.key == "homeX") | .value) = "41.6763"
    | (.. | objects | select(.key == "COMport") | .value) = "/dev/ttyACM0"' "$repo/defaultwebcontrol.json"
}

check_service() {
  local state user code body
  state="$(docker exec "$name" systemctl is-active webcontrol || true)"
  [ "$state" = active ] || fail "webcontrol.service is $state"
  user="$(docker exec "$name" bash -c 'ps -o user= -p "$(systemctl show -p MainPID --value webcontrol)"')"
  [ "$user" = pi ] || fail "service runs as '$user', not pi"
  for path in / /next/ /api/state; do
    code="$(http_code "$path")"
    [ "$code" = 200 ] || fail "GET $path returned $code"
  done
  body="$(as_pi "curl -s 'http://localhost:5000/socket.io/?EIO=4&transport=polling'")"
  case "$body" in 0\{*'"sid"'*) ;; *) fail "socket.io handshake answered: $body" ;; esac
}

check_settings() {
  local section key want got
  for section in maslow:motorSpacingX:3011.16237 advanced:homeX:41.6763 maslow:COMport:/dev/ttyACM0; do
    IFS=: read -r section key want <<<"$section"
    got="$(as_pi "curl -s http://localhost:5000/api/settings/$section" |
      jq -r --arg key "$key" '.settings[] | select(.key == $key) | .value')"
    [ "$got" = "$want" ] || fail "$key is '$got' after loading the copied ~/.WebControl, not $want"
  done
}

run_distro() {
  distro="$1"
  name="webcontrol-pi-test-$distro"
  say "Building the image"
  docker build -q --platform linux/arm64 --build-arg "DEBIAN=$distro" -t "$name" "$here" >/dev/null
  docker rm -f "$name" >/dev/null 2>&1 || true
  docker run -d --platform linux/arm64 --name "$name" --privileged --cgroupns=host \
    -v /sys/fs/cgroup:/sys/fs/cgroup:rw --tmpfs /run --tmpfs /run/lock "$name" >/dev/null
  docker exec "$name" systemctl is-system-running --wait >/dev/null || true

  say "Copying ~/.WebControl and the repo (the guide's steps 4 and 5)"
  as_pi 'mkdir -p ~/.WebControl'
  fixture_settings | docker exec -i -u pi "$name" tee /home/pi/.WebControl/webcontrol.json >/dev/null
  rsync -a "${excludes[@]}" -e "docker exec -i -u pi -w /home/pi" "$repo/" "$name:webcontrol-source/"

  say "Installing (the guide's step 6)"
  local start install_s used_before
  start=$SECONDS
  used_before="$(used_mb)"
  as_pi 'cd ~/webcontrol-source && tools/install_from_source.sh && tools/install_from_source.sh --activate' ||
    { fail "install_from_source.sh failed"; return; }
  install_s=$((SECONDS - start))

  say "Checking the service"
  check_service
  as_pi '~/webcontrol-source/.venv/bin/python -c "import cv2, scipy, numpy, gevent"' || fail "imports failed in .venv"
  check_settings
  docker exec "$name" systemctl restart webcontrol
  as_pi 'for i in $(seq 60); do curl -fs -o /dev/null http://localhost:5000/next/ && exit; sleep 1; done; exit 1' ||
    fail "no answer within 60s of systemctl restart"
  check_service
  check_settings
  sleep 20

  say "Resources and log"
  docker exec "$name" bash -c 'ps -o rss=,cmd= -p "$(systemctl show -p MainPID --value webcontrol)"' |
    awk '{printf "RSS: %d MB (%s %s)\n", $1 / 1024, $2, $3}'
  as_pi 'du -sh ~/webcontrol-source/.venv ~/.local/share/uv ~/.cache/uv ~/.local/bin/uv 2>/dev/null' | sed 's/^/disk: /'
  echo "disk: $(($(used_mb) - used_before)) MB in all, apt packages included"
  echo "install + activate: ${install_s}s"
  docker exec "$name" journalctl -u webcontrol --no-pager -o cat | tail -n 25 | sed 's/^/    log: /'
  ! docker exec "$name" journalctl -u webcontrol --no-pager -o cat | grep -E 'Traceback|Error|CRITICAL' ||
    fail "the log has errors (above)"
  [ -n "${KEEP:-}" ] || docker rm -f "$name" >/dev/null
}

[ -f "$repo/frontend/dist/index.html" ] || { echo "Build frontend/dist first: cd frontend && npm ci && npm run build" >&2; exit 1; }
distros=("$@")
[ $# -gt 0 ] || distros=(bookworm trixie)
for each in "${distros[@]}"; do run_distro "$each"; done
[ "$failures" = 0 ] || { echo "$failures check(s) failed"; exit 1; }
echo PASS
