#!/usr/bin/env bash
# Runs webcontrol-update for real in an arm64 (ARCH=x86_64: amd64) Debian container with systemd, against
# release.sh's tarballs served by a fake GitHub API: install, update, update over a backup, --status, no downgrade,
# --rollback, and a failed update, then the same from the UI over HTTP: POST /api/system/updates/install and /rollback, through the unit and sudoers.
# shellcheck disable=SC2016 # Single-quoted commands expand inside the container.
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: WC_FIRMWARE_HEX=<hex> rust/deploy/update-docker-test.sh 0.9.0=<binary> 0.9.1=<binary> 0.9.2=<binary>
Each binary is an aarch64 webcontrol-rs (x86_64 with ARCH=x86_64) built with that WC_VERSION.
KEEP=1 leaves the container running.
EOF
  exit 2
}

here="$(cd "$(dirname "$0")" && pwd)"
rust="$(dirname "$here")"
distro="${DEBIAN:-trixie}"
arch="${ARCH:-aarch64}"
platform="linux/$([ "$arch" = x86_64 ] && echo amd64 || echo arm64)"
name="webcontrol-update-test-$distro"
work="$(mktemp -d)"
failures=0
[ $# = 3 ] || usage
IFS='=' read -r v0 b0 <<< "$1"
IFS='=' read -r v1 b1 <<< "$2"
IFS='=' read -r v2 b2 <<< "$3"

cleanup() {
  rm -rf "$work"
  [ "${KEEP:-0}" = 1 ] || docker rm -f "$name" > /dev/null 2>&1 || true
}
trap cleanup EXIT

say() { printf '\n==> %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; failures=$((failures + 1)); }
as_pi() { docker exec -u pi -w /home/pi "$name" bash -c "$1"; }
# sudo needs pi's password, as on the Pi; one shared timestamp stands in for the terminal that would type it.
sudo_once() { as_pi 'echo raspberry | sudo -S -p "" -v'; }
updater() {
  sudo_once
  as_pi "WC_GITHUB_API=http://localhost:8080 WC_HEALTH_WAIT=${WAIT:-60} webcontrol-update $*"
}
installed() { as_pi '/opt/webcontrol-rs/webcontrol-rs --version' | cut -d' ' -f2; }
answering() { as_pi 'curl -fsS http://localhost:5000/api/state' | jq -r .version; }
manifest() { as_pi 'cat /opt/webcontrol-rs/backup/MANIFEST'; }
expect() { [ "$2" = "$3" ] || fail "$1 is \"$2\", not \"$3\""; }

# post <install|rollback>: the HTTP status, as the UI's button gets it; the body is left in /tmp/reply.
post() {
  as_pi "curl -s -o /tmp/reply -w '%{http_code}' -X POST -H 'Content-Type: application/json' -d '{}' \
    http://localhost:5000/api/system/updates/$1"
}
progress() { as_pi 'curl -fsS --max-time 3 http://localhost:5000/api/system/updates' 2> /dev/null | jq -c .progress; }

# Polls as the Updates screen does, through the restart, until the run has a result; prints the phases it saw.
finished_run() {
  local deadline=$((SECONDS + 300)) now seen=""
  while [ "$SECONDS" -lt "$deadline" ]; do
    now="$(progress || true)"
    if [ -n "$now" ] && [ "$now" != null ]; then
      seen="$seen $(jq -r .phase <<< "$now")"
      if [ "$(jq -r .running <<< "$now")" = false ]; then
        echo "$seen" > "$work/phases"
        echo "$now"
        return 0
      fi
    fi
    sleep 1
  done
  fail "the run started from the UI did not finish within 300 s"
  echo null
}

# release.sh packages both architectures; only $arch's tarball is installed, so the one binary stands in for both.
package() {
  local target="$work/target/$1" a
  for a in aarch64 x86_64; do
    mkdir -p "$target/$a-unknown-linux-gnu/release" && cp "$2" "$target/$a-unknown-linux-gnu/release/webcontrol-rs"
  done
  WC_SKIP_BUILD=1 CARGO_TARGET_DIR="$target" WC_VERSION="$1" bash "$rust/scripts/release.sh" "$work/gh/dl/v$1" > /dev/null
}

# publish <version>: the release JSON GitHub's API would give for it.
publish() {
  local v="$1" api="$work/gh/repos/designfrontier/WebControl/releases"
  mkdir -p "$api/tags"
  cat > "$api/tags/v$v" <<EOF
{"tag_name": "v$v", "name": "webcontrol-rs $v", "draft": false, "body": "Release $v",
 "html_url": "https://github.com/designfrontier/WebControl/releases/tag/v$v", "assets": [
  {"name": "webcontrol-rs-$v-$arch.tar.gz",
   "browser_download_url": "http://localhost:8080/dl/v$v/webcontrol-rs-$v-$arch.tar.gz"},
  {"name": "SHA256SUMS", "browser_download_url": "http://localhost:8080/dl/v$v/SHA256SUMS"}]}
EOF
  cp "$api/tags/v$v" "$api/latest"
  docker cp "$work/gh/." "$name:/srv/gh/"
}

say "Packaging $v0, $v1 and $v2 with scripts/release.sh"
package "$v0" "$b0"
package "$v1" "$b1"
package "$v2" "$b2"

say "Starting Debian $distro $platform with systemd"
docker build -q --platform "$platform" --build-arg "DEBIAN=$distro" -t "$name" "$here/pi-image" > /dev/null
docker rm -f "$name" > /dev/null 2>&1 || true
docker run -d --platform "$platform" --name "$name" --privileged --cgroupns=host \
  -v /sys/fs/cgroup:/sys/fs/cgroup:rw --tmpfs /run --tmpfs /run/lock "$name" > /dev/null
docker exec "$name" systemctl is-system-running --wait > /dev/null || true
docker exec "$name" bash -c 'apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq avrdude > /dev/null'
docker exec "$name" bash -c 'rm /etc/sudoers.d/010_pi-nopasswd && echo pi:raspberry | chpasswd &&
  echo "Defaults:pi timestamp_type=global" > /etc/sudoers.d/000_test && chmod 440 /etc/sudoers.d/000_test'
docker exec "$name" mkdir -p /srv/gh
docker exec "$name" systemd-run --unit fake-github python3 -m http.server 8080 --bind 127.0.0.1 --directory /srv/gh
docker cp "$work/gh/dl/v$v0/webcontrol-rs-$v0-$arch.tar.gz" "$name:/home/pi/"
docker exec "$name" chown pi:pi "/home/pi/webcontrol-rs-$v0-$arch.tar.gz"

say "Installing $v0 from its tarball, as the README says"
sudo_once
as_pi "tar xzf webcontrol-rs-$v0-$arch.tar.gz && bash webcontrol-rs-$v0-$arch/install-pi.sh webcontrol-rs-$v0-$arch"
docker exec "$name" bash -c 'mkdir -p /etc/systemd/system/webcontrol-rs.service.d &&
  printf "[Service]\nEnvironment=WC_GITHUB_API=http://localhost:8080\n" > /etc/systemd/system/webcontrol-rs.service.d/github.conf &&
  systemctl daemon-reload'
docker exec "$name" systemctl enable --now webcontrol-rs
as_pi 'for _ in $(seq 30); do curl -fs -o /dev/null http://localhost:5000/api/state && exit; sleep 1; done; exit 1' ||
  fail "$v0 did not answer"
expect "the version /api/state reports" "$(answering)" "$v0"

say "Up to date: nothing to do"
publish "$v0"
updater | tee "$work/out"
grep -q "Nothing to do" "$work/out" || fail "an up-to-date install was not left alone"

say "Updating $v0 to $v1 (no backup yet)"
publish "$v1"
updater | tee "$work/out" || fail "the update to $v1 failed"
expect "the installed version" "$(installed)" "$v1"
expect "the version /api/state reports" "$(answering)" "$v1"
manifest | grep -qx "version=$v0" || fail "the backup is not of $v0"
manifest | grep -qx "file=/home/pi/.WebControl/webcontrol.json" || fail "the backup lacks webcontrol.json"
expect "the files backed up" "$(manifest | grep -c '^file=')" 8
as_pi '/opt/webcontrol-rs/backup/files/opt/webcontrol-rs/webcontrol-rs --version' | grep -q " $v0 " ||
  fail "the backed-up binary is not $v0"

say "Updating $v1 to $v2 over the existing backup"
publish "$v2"
updater | tee "$work/out" || fail "the update to $v2 failed"
grep -q "Deleting the previous backup (webcontrol-rs $v0" "$work/out" || fail "the old backup was not cleared"
expect "the installed version" "$(installed)" "$v2"
expect "the version /api/state reports" "$(answering)" "$v2"
manifest | grep -qx "version=$v1" || fail "the backup is not of $v1"

say "--status"
updater --status | tee "$work/out"
grep -q "Backup: *webcontrol-rs $v1" "$work/out" || fail "--status does not show the backup"
grep -q "Latest release: *v$v2" "$work/out" || fail "--status does not show the latest release"

say "A latest release older than $v2: nothing changes, and sudo is not asked for"
publish "$v1"
before="$(manifest)"
as_pi "sudo -K && WC_GITHUB_API=http://localhost:8080 webcontrol-update < /dev/null" | tee "$work/out" ||
  fail "refusing a downgrade failed"
grep -q "Installed $v2 is newer than the latest published release $v1; nothing to do" "$work/out" ||
  fail "an older latest release was not refused"
expect "the installed version" "$(installed)" "$v2"
expect "the backup" "$(manifest)" "$before"
updater --status | tee "$work/out"
grep -q "Update: *installed $v2 is newer than latest published $v1" "$work/out" || fail "--status does not say $v2 is newer"
publish "$v2"

say "--rollback to $v1"
updater --rollback | tee "$work/out" || fail "--rollback failed"
expect "the installed version" "$(installed)" "$v1"
expect "the version /api/state reports" "$(answering)" "$v1"
manifest | grep -qx "version=$v1" || fail "--rollback did not keep the backup"

say "A $v2 whose unit cannot start: the update rolls back to $v1 by itself"
as_pi "mkdir -p bad && tar -C bad -xzf /srv/gh/dl/v$v2/webcontrol-rs-$v2-$arch.tar.gz &&
  sed -i 's|--port 5000|--port 5000 --no-such-flag|' bad/webcontrol-rs-$v2-$arch/webcontrol-rs.service &&
  tar -C bad -czf bad.tar.gz webcontrol-rs-$v2-$arch && sha256sum bad.tar.gz | cut -d' ' -f1"
docker exec "$name" bash -c "cp /home/pi/bad.tar.gz /srv/gh/dl/v$v2/webcontrol-rs-$v2-$arch.tar.gz &&
  cd /srv/gh/dl/v$v2 && sha256sum webcontrol-rs-$v2-$arch.tar.gz > SHA256SUMS"
if WAIT=20 updater | tee "$work/out"; then fail "a broken update reported success"; fi
grep -q "ROLLED BACK: webcontrol-rs $v1 is running again" "$work/out" || fail "no automatic rollback"
expect "the installed version" "$(installed)" "$v1"
expect "the version /api/state reports" "$(answering)" "$v1"
as_pi 'grep -q -- "--no-such-flag" /etc/systemd/system/webcontrol-rs.service' && fail "the broken unit was not restored"
expect "the service" "$(docker exec "$name" systemctl is-active webcontrol-rs || true)" active
expect "webcontrol.json's owner" "$(docker exec "$name" stat -c %U /home/pi/.WebControl/webcontrol.json)" pi
expect "the sudoers rule's owner and mode" "$(docker exec "$name" stat -c '%U %a' /etc/sudoers.d/webcontrol-rs)" "root 440"
docker exec "$name" visudo -c > /dev/null || fail "sudoers does not parse after the rollback"

say "From the UI: POST /api/system/updates/install updates $v1 to $v2, with no sudo password"
publish "$v2"
docker exec "$name" systemctl restart webcontrol-rs
as_pi 'for _ in $(seq 30); do curl -fs -o /dev/null http://localhost:5000/api/state && exit; sleep 1; done; exit 1'
as_pi 'sudo -K'
expect "canUpdate" "$(as_pi 'curl -fsS http://localhost:5000/api/system/updates' | jq -r .canUpdate)" true
expect "the install answer" "$(post install)" 202
run="$(finished_run)"
cat "$work/phases"
expect "the run's action and result" "$(jq -r '"\(.action) \(.result)"' <<< "$run")" "update updated"
expect "the run's versions" "$(jq -r '"\(.from) \(.to)"' <<< "$run")" "$v1 $v2"
grep -qE "starting|checking|downloading|installing|restarting|healthCheck" "$work/phases" || fail "no progress was seen while it ran"
expect "the installed version" "$(installed)" "$v2"
expect "the version /api/state reports" "$(answering)" "$v2"
manifest | grep -qx "version=$v1" || fail "the UI's update did not back up $v1"
expect "the status file's owner and mode" "$(docker exec "$name" stat -c '%U %a' /var/lib/webcontrol-rs/update.json)" "root 644"
expect "the unit's result" "$(docker exec "$name" systemctl show -p Result --value webcontrol-update@update)" success
expect "a second install answer" "$(post install)" 409
as_pi 'cat /tmp/reply' | grep -q "no newer published release" || fail "the second install was not refused for having nothing newer"

say "Without a password, the sudoers rule allows those two starts exactly, and nothing more"
# pi is in the sudo group, so anything else asks for the password, which -n refuses to do.
needs_password() { [[ "$(as_pi "sudo -n /usr/bin/systemctl $1" 2>&1)" == *"password is required"* ]]; }
for command in "start --no-block webcontrol-update@update.service --now" "start --no-block webcontrol-update@frob.service" \
  "start --no-block webcontrol-update@update.service webcontrol-rs.service" "start webcontrol-update@update.service" \
  "stop webcontrol-rs" "start --no-block webcontrol-update@*"; do
  needs_password "$command" || fail "sudo allows systemctl $command without a password"
done

say "From the UI: POST /api/system/updates/rollback puts $v1 back, without following a link pi planted"
docker exec "$name" bash -c 'echo untouched > /etc/wc-target && chmod 644 /etc/wc-target'
as_pi 'ln -sf /etc/wc-target .WebControl/webcontrol.json.wc-restore'
expect "the rollback answer" "$(post rollback)" 202
run="$(finished_run)"
expect "the run's action and result" "$(jq -r '"\(.action) \(.result)"' <<< "$run")" "rollback rolledBack"
expect "the installed version" "$(installed)" "$v1"
expect "the version /api/state reports" "$(answering)" "$v1"
expect "the file pi's link pointed at" "$(docker exec "$name" stat -c '%U' /etc/wc-target) $(docker exec "$name" cat /etc/wc-target)" \
  "root untouched"
expect "webcontrol.json's owner and type" "$(docker exec "$name" stat -c '%U %F' /home/pi/.WebControl/webcontrol.json)" \
  "pi regular file"

say "From the UI: a $v2 that cannot start rolls back by itself, and the page is told"
docker exec "$name" bash -c "cp /home/pi/bad.tar.gz /srv/gh/dl/v$v2/webcontrol-rs-$v2-$arch.tar.gz &&
  cd /srv/gh/dl/v$v2 && sha256sum webcontrol-rs-$v2-$arch.tar.gz > SHA256SUMS"
expect "the install answer" "$(post install)" 202
run="$(finished_run)"
expect "the run's action and result" "$(jq -r '"\(.action) \(.result)"' <<< "$run")" "update rolledBack"
jq -e '.log | index("ROLLED BACK: webcontrol-rs '"$v1"' is running again, with its settings.")' <<< "$run" > /dev/null ||
  fail "the log the page shows lacks the rollback"
expect "the installed version" "$(installed)" "$v1"
expect "the version /api/state reports" "$(answering)" "$v1"
docker exec "$name" visudo -c > /dev/null || fail "sudoers does not parse after the UI's runs"

if [ "$failures" != 0 ]; then
  echo "$failures failed"
  exit 1
fi
echo "webcontrol-update passed in Debian $distro $platform with systemd"
