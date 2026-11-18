#!/usr/bin/env bash
# Tests webcontrol-update against release.sh's real tarballs, a file:// GitHub API and /api/state, and stub
# sudo, systemctl, uname, id and webcontrol-rs binaries; nothing touches a real system.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
rust="$(dirname "$here")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
real_curl="$(command -v curl)"
failures=0
mkdir -p "$work/bin" "$work/releases"

printf ':00000001FF\n' > "$work/fw.hex"
(sha256sum "$work/fw.hex" 2> /dev/null || shasum -a 256 "$work/fw.hex") | cut -d' ' -f1 > "$work/fw.sha256"

# Every stub logs to $work/calls. sudo runs its command as is, except cp when $work/fail-cp exists.
cat > "$work/bin/sudo" <<EOF
#!/usr/bin/env bash
echo "sudo \$*" >> "$work/calls"
[ "\$1" = -v ] && exit 0
[ "\$1" = cp ] && [ -f "$work/fail-cp" ] && exit 1
exec "\$@"
EOF
cat > "$work/bin/visudo" <<EOF
#!/usr/bin/env bash
! grep -qs "BAD SUDOERS" "\${*: -1}"
EOF
# With \$work/as-root, everything that asks id -u runs as root would.
cat > "$work/bin/id" <<EOF
#!/usr/bin/env bash
if [ "\$1" = -u ] && [ -f "$work/as-root" ]; then echo 0; else exec /usr/bin/id "\$@"; fi
EOF
# A stub that records the update's phase when it is called, so a test sees the phases in order.
phase_probe() {
  printf '%s\n' "[ -f \"$work/root/var/lib/webcontrol-rs/update.json\" ] &&" \
    "  sed -n 's/.*\"phase\":\"\\([A-Za-z]*\\)\".*/\\1/p' \"$work/root/var/lib/webcontrol-rs/update.json\" >> \"$work/phases\""
}
cat > "$work/bin/uname" <<EOF
#!/usr/bin/env bash
if [ "\$1" = -m ]; then cat "$work/arch"; else exec /usr/bin/uname "\$@"; fi
EOF
cat > "$work/bin/curl" <<EOF
#!/usr/bin/env bash
echo "curl \$*" >> "$work/calls"
$(phase_probe)
[[ " \$* " == *" -K - "* ]] || exec "$real_curl" "\$@"
config="\$(cat)"
echo "curl-config \$config" >> "$work/calls"
printf '%s\n' "\$config" | exec "$real_curl" "\$@"
EOF
# A BROKEN binary never answers and scribbles on webcontrol.json when stopped. A working one answers with its version
# (or stale-version) and the controller (unless controller-lost); with job-starts, install-pi.sh's reload starts a job.
cat > "$work/bin/systemctl" <<EOF
#!/usr/bin/env bash
echo "systemctl \$*" >> "$work/calls"
$(phase_probe)
bin="$work/root/opt/webcontrol-rs/webcontrol-rs"
case "\$1" in
  daemon-reload)
    [ -f "$work/job-starts" ] && [ -f "$work/srv/api/state" ] &&
      sed -i.orig 's/"jobInProgress":false/"jobInProgress":true/' "$work/srv/api/state" &&
      echo '{"owner": "changed during the job"}' > "$work/home/.WebControl/webcontrol.json" ;;
  show) cat "$work/service-env" 2> /dev/null ;;
  is-active)
    s=inactive
    [ "\${*: -1}" != webcontrol-rs ] || s="\$(cat "$work/active" 2> /dev/null || echo inactive)"
    [ "\$2" = --quiet ] || echo "\$s"
    [ "\$s" = active ]
    exit ;;
  restart | start | stop)
    if grep -qs BROKEN "$work/running"; then echo '{"scribbled": true}' > "$work/home/.WebControl/webcontrol.json"; fi
    rm -f "$work/srv/api/state" "$work/running"
    echo inactive > "$work/active"
    [ "\$1" = stop ] && exit 0
    cp "\$bin" "$work/running"
    echo active > "$work/active"
    grep -q BROKEN "\$bin" && exit 0
    v="\$(cat "$work/stale-version" 2> /dev/null || "\$bin" --version | cut -d' ' -f2)"
    fw="\$(cat "$work/controller" 2> /dev/null || echo 0)"
    [ -f "$work/controller-lost" ] && fw=0
    printf '{"backend":"rust","jobInProgress":false,"pauseButtonSetting":"Pause","manualZAxisAdjust":false,"probing":false,"controllerFirmwareVersion":%s,"version":"%s"}' \
      "\$fw" "\$v" > "$work/srv/api/state" ;;
esac
exit 0
EOF
chmod +x "$work/bin/"*

# release <version> [BROKEN]: release.sh's assets for a stub binary, cached in $work/releases/<version>.
release() {
  local v="$1" out="$work/releases/$1"
  [ -d "$out" ] && return 0
  printf '#!/bin/sh\n# :00000001FF %s\necho "webcontrol-rs %s (abc1234, contract 2)"\n' "${2:-}" "$v" > "$work/stub"
  chmod +x "$work/stub"
  env -u GITHUB_REF_TYPE -u GITHUB_REF_NAME -u GITHUB_OUTPUT WC_SKIP_BUILD=1 WC_BINARY="$work/stub" \
    WC_FIRMWARE_HEX="$work/fw.hex" WC_FIRMWARE_PIN="$work/fw.sha256" WC_VERSION="$v" \
    bash "$rust/scripts/release.sh" "$out" > /dev/null
}

api="$work/gh/repos/designfrontier/WebControl/releases"

# publish <version> [BROKEN]: a published release as GitHub's API describes it.
publish() {
  local v="$1" dl="file://$work/releases/$1"
  release "$@"
  mkdir -p "$api/tags"
  cat > "$api/tags/v$v" <<EOF
{
  "tag_name": "v$v",
  "draft": false,
  "assets": [
    {"name": "webcontrol-rs-$v-aarch64Xtar.gz", "browser_download_url": "$dl/webcontrol-rs-$v-aarch64Xtar.gz"},
    {"name": "webcontrol-rs-$v-aarch64.tar.gz", "browser_download_url": "$dl/webcontrol-rs-$v-aarch64.tar.gz"},
    {"name": "webcontrol-rs-$v-aarch64", "browser_download_url": "$dl/webcontrol-rs-$v-aarch64"},
    {"name": "SHA256SUMS", "browser_download_url": "$dl/SHA256SUMS"}
  ]
}
EOF
}

latest() { cp "$api/tags/v$1" "$api/latest"; }

# installed <version>: that release installed by its own install-pi.sh, running, with the owner's settings.
installed() {
  release "$1"
  tar -C "$work/x" -xzf "$work/releases/$1/webcontrol-rs-$1-aarch64.tar.gz"
  env PATH="$work/bin:$PATH" WC_ROOT="$work/root" WC_USER=pi WC_APT=true \
    bash "$work/x/webcontrol-rs-$1-aarch64/install-pi.sh" "$work/x/webcontrol-rs-$1-aarch64" > /dev/null
  PATH="$work/bin:$PATH" systemctl restart webcontrol-rs
  rm -f "$work/calls"
}

fresh() {
  rm -rf "${work:?}/root" "${work:?}/home" "$work/srv" "$work/gh" "$work/x" "$work/calls" "$work/active" \
    "$work/as-root" "$work/service-env" "$work/phases" "$work/controller" "$work/controller-lost" "$work/stale-version" "$work/running" "$work/out" "$work/err" \
    "$work/fail-cp" "$work/job-starts"
  mkdir -p "$work/root" "$work/home/.WebControl" "$work/srv/api" "$work/x"
  echo '{"owner": "settings"}' > "$work/home/.WebControl/webcontrol.json"
  echo aarch64 > "$work/arch"
}

# update [NAME=value...] [flags...]
update() {
  local vars=()
  while [[ "${1:-}" =~ ^[A-Z_]+= ]]; do
    vars+=("$1")
    shift
  done
  env -u GITHUB_TOKEN -u WEBCONTROL_REPO PATH="$work/bin:$PATH" WC_ROOT="$work/root" WC_HOME="$work/home" \
    WC_URL="file://$work/srv" WC_GITHUB_API="file://$work/gh" WC_HEALTH_WAIT=1 WC_POLL=0.1 WC_APT=true \
    ${vars[@]+"${vars[@]}"} bash "$rust/webcontrol-update" "$@"
}

opt="$work/root/opt/webcontrol-rs"
backup="$opt/backup"
version_of() { "$1" --version | cut -d' ' -f2; }
snapshot() { (cd "$work" && find root home -type f -exec cksum {} + | sort); }
no_changes_since() { [ "$(snapshot)" = "$1" ] && ! grep -qs "systemctl restart" "$work/calls"; }

check() {
  local name="$1"
  shift
  if "$@"; then echo "ok   $name"; else echo "FAIL $name"; failures=$((failures + 1)); fi
}

up_to_date() {
  fresh
  installed 1.0
  publish 1.0
  latest 1.0
  local before
  before="$(snapshot)"
  update > "$work/out" && grep -q "1.0 is installed, the latest published release" "$work/out" &&
    no_changes_since "$before" && ! grep -q "sudo" "$work/calls"
}

update_without_a_backup() {
  fresh
  installed 1.0
  echo 51.29 > "$work/controller"
  systemctl_restart
  publish 1.1
  latest 1.1
  update > "$work/out" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.1 ] &&
    grep -q "Updated webcontrol-rs to 1.1" "$work/out" && grep -q "controller connected" "$work/out" &&
    ! grep -q "Deleting the previous backup" "$work/out" &&
    [ "$(grep -c "^sudo -v" "$work/calls")" = 1 ] && grep -q "systemctl restart webcontrol-rs" "$work/calls" &&
    grep -qx "version=1.0" "$backup/MANIFEST" && grep -qE "^date=20[0-9-]+T" "$backup/MANIFEST" &&
    [ "$(version_of "$backup/files$opt/webcontrol-rs")" = 1.0 ] &&
    cmp -s "$backup/files$work/home/.WebControl/webcontrol.json" "$work/home/.WebControl/webcontrol.json" &&
    [ "$(grep -c "^file=" "$backup/MANIFEST")" = 8 ] &&
    grep -qx "file=$work/root/etc/systemd/system/webcontrol-update@.service" "$backup/MANIFEST" &&
    grep -qx "file=$work/root/etc/sudoers.d/webcontrol-rs" "$backup/MANIFEST" &&
    grep -qx "file=$work/root/etc/systemd/system/webcontrol.service.d/conflicts.conf" "$backup/MANIFEST" &&
    grep -qx "file=$work/root/usr/local/bin/webcontrol-update" "$backup/MANIFEST" &&
    grep -qx "file=$work/root/usr/local/bin/wc-switch" "$backup/MANIFEST" &&
    grep -qx "file=$work/root/etc/systemd/system/webcontrol-rs.service" "$backup/MANIFEST"
}

systemctl_restart() { PATH="$work/bin:$PATH" systemctl restart webcontrol-rs; rm -f "$work/calls"; }

update_over_a_backup() {
  fresh
  installed 1.0
  publish 1.1
  publish 1.2
  latest 1.1
  update > /dev/null || return 1
  touch "$backup/files/stale"
  latest 1.2
  update > "$work/out" &&
    grep -q "Deleting the previous backup (webcontrol-rs 1.0, made 20" "$work/out" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.2 ] &&
    grep -qx "version=1.1" "$backup/MANIFEST" && [ ! -e "$backup/files/stale" ] &&
    [ "$(version_of "$backup/files$opt/webcontrol-rs")" = 1.1 ]
}

first_install_skips_the_backup() {
  fresh
  publish 1.1
  latest 1.1
  update > "$work/out" && grep -q "nothing to back up" "$work/out" &&
    grep -q "not running, so no job is; carrying on" "$work/out" && [ "$(version_of "$opt/webcontrol-rs")" = 1.1 ] && [ ! -e "$backup" ]
}

checksum_mismatch_changes_nothing() {
  fresh
  installed 1.0
  publish 1.1
  latest 1.1
  update > /dev/null || return 1
  publish 1.3
  latest 1.3
  sed -i.orig 's/^[0-9a-f]\{8\}/00000000/' "$work/releases/1.3/SHA256SUMS"
  local before
  before="$(snapshot)"
  rm -f "$work/calls"
  ! update > "$work/out" 2> "$work/err" && grep -q "Checksum mismatch" "$work/err" &&
    grep -q "Nothing was changed" "$work/err" && no_changes_since "$before" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.1 ] && grep -qx "version=1.0" "$backup/MANIFEST"
}

refuses_a_tarball_without_install_files() {
  fresh
  installed 1.0
  publish 1.4
  latest 1.4
  local v="$work/releases/1.4" before
  mkdir "$work/y" && tar -C "$work/y" -xzf "$v/webcontrol-rs-1.4-aarch64.tar.gz" &&
    rm "$work/y/webcontrol-rs-1.4-aarch64/install-pi.sh" &&
    tar -C "$work/y" -czf "$v/webcontrol-rs-1.4-aarch64.tar.gz" webcontrol-rs-1.4-aarch64 && rm -rf "$work/y"
  (cd "$v" && (sha256sum webcontrol-rs-1.4-aarch64.tar.gz 2> /dev/null ||
    shasum -a 256 webcontrol-rs-1.4-aarch64.tar.gz) > SHA256SUMS)
  before="$(snapshot)"
  ! update 2> "$work/err" > /dev/null && grep -q "has no install-pi.sh" "$work/err" && no_changes_since "$before"
  local result=$?
  rm -rf "$v"
  return "$result"
}

refuses_while_busy() {
  local busy
  for busy in '"jobInProgress":true' '"pauseButtonSetting":"Resume"' '"manualZAxisAdjust":true' '"probing":true'; do
    local key="${busy%%:*}"
    fresh
    installed 1.0
    publish 1.1
    latest 1.1
    sed -i.orig "s/$key:[^,}]*/$busy/" "$work/srv/api/state"
    local before
    before="$(snapshot)"
    if ! { ! update > /dev/null 2> "$work/err" && grep -q "Not now" "$work/err" && no_changes_since "$before" &&
      ! grep -q "releases/latest" "$work/calls"; }; then
      echo "  not refused with $busy" >&2
      return 1
    fi
  done
}

refuses_during_a_firmware_upgrade() {
  fresh
  installed 1.0
  publish 1.1
  latest 1.1
  mkdir -p "$work/srv/api/system"
  echo '{"firmwareUpgrade": {"target": "51.292", "stage": "flashing", "message": ""}}' > "$work/srv/api/system/info"
  ! update > /dev/null 2> "$work/err" && grep -q "firmware upgrade is underway (flashing)" "$work/err" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.0 ]
}

# The installed 1.0 files carry a marker each, so a restore is told from 1.1's install of the same files.
mark_installed_files() {
  local f
  for f in etc/systemd/system/webcontrol-rs.service etc/systemd/system/webcontrol.service.d/conflicts.conf \
    usr/local/bin/wc-switch usr/local/bin/webcontrol-update etc/sudoers.d/webcontrol-rs; do
    chmod u+w "$work/root/$f"
    echo "# installed by 1.0" >> "$work/root/$f"
  done
}

all_marked() {
  local f
  for f in etc/systemd/system/webcontrol-rs.service etc/systemd/system/webcontrol.service.d/conflicts.conf \
    usr/local/bin/wc-switch usr/local/bin/webcontrol-update etc/sudoers.d/webcontrol-rs; do
    grep -q "^# installed by 1.0" "$work/root/$f" || { echo "  $f was not restored" >&2; return 1; }
  done
}

failed_health_check_rolls_back() {
  fresh
  installed 1.0
  mark_installed_files
  publish 1.9 BROKEN
  latest 1.9
  ! update > "$work/out" 2>&1 && grep -q "Health check failed" "$work/out" &&
    grep -q "ROLLED BACK: webcontrol-rs 1.0 is running again" "$work/out" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.0 ] && all_marked &&
    grep -qx '{"owner": "settings"}' "$work/home/.WebControl/webcontrol.json" &&
    grep -q '"version":"1.0"' "$work/srv/api/state" && [ "$(cat "$work/active")" = active ] &&
    grep -qx "version=1.0" "$backup/MANIFEST"
}

stale_version_rolls_back() {
  fresh
  installed 1.0
  publish 1.1
  latest 1.1
  echo 1.0 > "$work/stale-version"
  ! update > "$work/out" 2>&1 && grep -q 'version "1.0" (wanted rust, 1.1)' "$work/out" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.0 ]
}

lost_controller_rolls_back() {
  fresh
  echo 51.29 > "$work/controller"
  installed 1.0
  publish 1.1
  latest 1.1
  touch "$work/controller-lost"
  ! update > "$work/out" 2>&1 && grep -q "the controller did not reconnect" "$work/out" &&
    grep -q "ROLLBACK FAILED" "$work/out" && grep -q "SSD or the old card" "$work/out" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.0 ]
}

no_controller_only_warns() {
  fresh
  installed 1.0
  publish 1.1
  latest 1.1
  update > "$work/out" && grep -q "Warning: no controller is connected" "$work/out" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.1 ]
}

manual_rollback() {
  fresh
  installed 1.0
  mark_installed_files
  publish 1.1
  latest 1.1
  update > /dev/null || return 1
  echo '{"owner": "changed by 1.1"}' > "$work/home/.WebControl/webcontrol.json"
  rm -f "$work/calls"
  update --rollback > "$work/out" && grep -q "ROLLED BACK: webcontrol-rs 1.0" "$work/out" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.0 ] && all_marked &&
    grep -qx '{"owner": "settings"}' "$work/home/.WebControl/webcontrol.json" &&
    grep -q "systemctl restart webcontrol-rs" "$work/calls" && grep -q '"version":"1.0"' "$work/srv/api/state" &&
    grep -qx "version=1.0" "$backup/MANIFEST" && [ -f "$backup/files$opt/webcontrol-rs" ]
}

rollback_refuses_while_busy_or_without_a_backup() {
  fresh
  installed 1.0
  ! update --rollback > /dev/null 2> "$work/err" && grep -q "no backup" "$work/err" || return 1
  publish 1.1
  latest 1.1
  update > /dev/null || return 1
  sed -i.orig 's/"jobInProgress":false/"jobInProgress":true/' "$work/srv/api/state"
  ! update --rollback > /dev/null 2> "$work/err" && grep -q "Not now" "$work/err" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.1 ]
}

status_shows_versions() {
  fresh
  installed 1.0
  publish 1.1
  publish 1.2
  latest 1.1
  update > /dev/null || return 1
  latest 1.2
  rm -f "$work/calls"
  update --status > "$work/out" && grep -qE "^Installed: +webcontrol-rs 1.1$" "$work/out" &&
    grep -qE "^Backup: +webcontrol-rs 1.0, made 20[0-9-]+T.*, in $backup$" "$work/out" &&
    grep -qE "^Latest release: +v1.2 \(designfrontier/WebControl\)$" "$work/out" &&
    grep -qE "^Service: +active$" "$work/out" && ! grep -q sudo "$work/calls"
}

status_without_releases_or_backup() {
  fresh
  update --status > "$work/out" && grep -qE "^Installed: +none$" "$work/out" &&
    grep -qE "^Backup: +none$" "$work/out" && grep -q "none published .*(drafts are not visible)" "$work/out"
}

pins_a_version() {
  fresh
  installed 1.0
  publish 1.1
  publish 1.2
  latest 1.2
  update --version v1.1 > /dev/null && [ "$(version_of "$opt/webcontrol-rs")" = 1.1 ] &&
    grep -q "releases/tags/v1.1" "$work/calls" || return 1
  update --version 1.1 > "$work/out" && grep -q "1.1 is installed. Nothing to do" "$work/out" &&
    update --version=1.1 --force > /dev/null && grep -qx "version=1.1" "$backup/MANIFEST"
}

drafts_are_invisible() {
  fresh
  installed 1.0
  ! update > /dev/null 2> "$work/err" && grep -q "No published release found in designfrontier/WebControl" "$work/err" &&
    grep -q "Drafts are not visible" "$work/err"
}

refuses_off_aarch64() {
  fresh
  installed 1.0
  publish 1.1
  latest 1.1
  echo x86_64 > "$work/arch"
  local before
  before="$(snapshot)"
  ! update > /dev/null 2> "$work/err" && grep -q "aarch64); this machine is x86_64" "$work/err" &&
    no_changes_since "$before" && ! update --rollback > /dev/null 2>&1
}

takes_the_repo_and_token() {
  fresh
  installed 1.0
  mkdir -p "$work/gh/repos/maker/fork/releases"
  publish 1.1
  cp "$api/tags/v1.1" "$work/gh/repos/maker/fork/releases/latest"
  update GITHUB_TOKEN=secret --repo maker/fork > /dev/null &&
    grep -qx 'curl-config header = "Authorization: Bearer secret"' "$work/calls" &&
    grep -q "^curl -K - .*repos/maker/fork/releases/latest" "$work/calls" &&
    ! grep "^curl " "$work/calls" | grep -q secret && ! grep "SHA256SUMS" "$work/calls" | grep -q -- "-K" || return 1
  fresh
  installed 1.0
  mkdir -p "$work/gh/repos/maker/fork/releases"
  publish 1.1
  cp "$api/tags/v1.1" "$work/gh/repos/maker/fork/releases/latest"
  update WEBCONTROL_REPO=maker/fork > /dev/null && [ "$(version_of "$opt/webcontrol-rs")" = 1.1 ] &&
    ! update --repo 'x;y' > /dev/null 2>&1
}

status="$work/root/var/lib/webcontrol-rs/update.json"
# progress <jq filter>: true when the status file the server reads passes it.
progress() { jq -e "$1" "$status" > /dev/null; }
mode_of() { stat -c %a "$1" 2> /dev/null || stat -f %Lp "$1"; }

service_update_reports_its_progress() {
  fresh
  installed 1.0
  publish 1.1
  latest 1.1
  touch "$work/as-root"
  update --service update > "$work/out" && [ "$(version_of "$opt/webcontrol-rs")" = 1.1 ] &&
    ! grep -q "^sudo" "$work/calls" && grep -qx "version=1.0" "$backup/MANIFEST" &&
    progress '.action == "update" and .phase == "finished" and .result == "updated"' &&
    progress '.from == "1.0" and .to == "1.1" and .startedAt > 1700000000000 and .finishedAt >= .startedAt' &&
    progress '(.log | index("Checksum OK.")) and .log[-2] == "Updated webcontrol-rs to 1.1."' &&
    [ "$(uniq "$work/phases" | tr '\n' ' ')" = "checking downloading installing restarting healthCheck " ] &&
    [ "$(mode_of "$status")" = 644 ] && [ ! -e "$status.new" ]
}

service_reports_an_automatic_rollback() {
  fresh
  installed 1.0
  publish 1.9 BROKEN
  latest 1.9
  touch "$work/as-root"
  ! update --service update > /dev/null 2>&1 && [ "$(version_of "$opt/webcontrol-rs")" = 1.0 ] &&
    progress '.action == "update" and .result == "rolledBack" and .to == "1.9"' &&
    progress '.log | index("ROLLED BACK: webcontrol-rs 1.0 is running again, with its settings.")' &&
    grep -qx rollingBack "$work/phases"
}

service_reports_a_failed_rollback() {
  fresh
  echo 51.29 > "$work/controller"
  installed 1.0
  publish 1.1
  latest 1.1
  touch "$work/controller-lost" "$work/as-root"
  ! update --service update > /dev/null 2>&1 && progress '.result == "rollbackFailed" and .phase == "finished"'
}

service_rolls_back() {
  fresh
  installed 1.0
  publish 1.1
  latest 1.1
  touch "$work/as-root"
  update --service update > /dev/null || return 1
  update --service rollback > /dev/null && [ "$(version_of "$opt/webcontrol-rs")" = 1.0 ] &&
    progress '.action == "rollback" and .result == "rolledBack" and .from == "1.1" and .to == "1.0"' &&
    ! grep -q "^sudo" "$work/calls"
}

service_reports_a_refusal() {
  fresh
  installed 1.0
  publish 1.1
  latest 1.1
  touch "$work/as-root"
  sed -i.orig 's/"jobInProgress":false/"jobInProgress":true/' "$work/srv/api/state"
  ! update --service update > /dev/null 2>&1 && [ "$(version_of "$opt/webcontrol-rs")" = 1.0 ] &&
    progress '.result == "failed" and .from == null' &&
    progress '.log[-1] == "Not now: a job is running or paused. Finish or stop it, then run webcontrol-update again."' &&
    ! update --service rollback > /dev/null 2>&1 && progress '.action == "rollback" and .result == "failed"'
}

service_up_to_date() {
  fresh
  installed 1.1
  publish 1.1
  latest 1.1
  touch "$work/as-root"
  update --service update > /dev/null && progress '.result == "upToDate" and .from == "1.1" and .to == "1.1"'
}

service_escapes_the_log() {
  fresh
  installed 1.0
  mkdir -p "$api"
  printf '{"tag_name": "bad\\\\x\t1"}' > "$api/latest"
  touch "$work/as-root"
  ! update --service update > /dev/null 2>&1 &&
    progress '.log[-1] == "The release'"'"'s tag \"bad\\\\x 1\" is not a version."'
}

service_takes_webcontrol_rs_environment() {
  fresh
  installed 1.0
  mkdir -p "$work/gh/repos/maker/fork/releases"
  publish 1.1
  cp "$api/tags/v1.1" "$work/gh/repos/maker/fork/releases/latest"
  echo "WEBCONTROL_REPO=maker/fork GITHUB_TOKEN=secret" > "$work/service-env"
  touch "$work/as-root"
  update WEBCONTROL_REPO=other/repo --service update > /dev/null && [ "$(version_of "$opt/webcontrol-rs")" = 1.1 ] &&
    grep -q "^curl -K - .*repos/maker/fork/releases/latest" "$work/calls" &&
    grep -qx 'curl-config header = "Authorization: Bearer secret"' "$work/calls"
}

service_needs_root_and_one_action() {
  fresh
  installed 1.0
  ! update --service update > /dev/null 2> "$work/err" && grep -q "runs as root" "$work/err" || return 1
  touch "$work/as-root"
  ! update --service frob > /dev/null 2>&1 && ! update --service update --force > /dev/null 2>&1 &&
    ! update --service > /dev/null 2>&1 && [ ! -e "$status" ]
}

service_runs_one_at_a_time() {
  command -v flock > /dev/null || return 0
  fresh
  installed 1.0
  publish 1.1
  latest 1.1
  touch "$work/as-root"
  mkdir -p "$(dirname "$status")"
  echo '{"phase": "downloading"}' > "$status"
  { flock -n 9 && ! update --service update > /dev/null 2> "$work/err"; } 9> "$(dirname "$status")/update.lock" &&
    grep -q "already running" "$work/err" && progress '.phase == "downloading"' &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.0 ]
}

terminal_runs_write_no_status() {
  fresh
  installed 1.0
  publish 1.1
  latest 1.1
  update > /dev/null && [ ! -e "$status" ] && [ "$(grep -c "^sudo -v" "$work/calls")" = 1 ]
}

hung_service_is_busy() {
  fresh
  installed 1.0
  publish 1.1
  latest 1.1
  rm "$work/srv/api/state"
  local before
  before="$(snapshot)"
  ! update > /dev/null 2> "$work/err" && grep -q "running but not answering" "$work/err" && no_changes_since "$before"
}

failed_backup_keeps_the_old_one() {
  fresh
  installed 1.0
  publish 1.1
  publish 1.2
  latest 1.1
  update > /dev/null || return 1
  latest 1.2
  touch "$work/fail-cp"
  ! update > /dev/null 2>&1 && grep -qx "version=1.0" "$backup/MANIFEST" &&
    [ "$(version_of "$backup/files$opt/webcontrol-rs")" = 1.0 ] && [ "$(version_of "$opt/webcontrol-rs")" = 1.1 ]
}

job_started_during_the_update() {
  fresh
  installed 1.0
  mark_installed_files
  publish 1.1
  latest 1.1
  touch "$work/job-starts"
  ! update > "$work/out" 2>&1 && grep -q "Not restarting: a job is running" "$work/out" &&
    ! grep -q "systemctl restart" "$work/calls" && [ "$(version_of "$opt/webcontrol-rs")" = 1.0 ] && all_marked &&
    grep -qx '{"owner": "changed during the job"}' "$work/home/.WebControl/webcontrol.json" &&
    grep -q '"version":"1.0"' "$work/srv/api/state"
}

restore_skips_a_sudoers_rule_visudo_rejects() {
  fresh
  installed 1.0
  mark_installed_files
  echo "BAD SUDOERS" >> "$work/root/etc/sudoers.d/webcontrol-rs"
  publish 1.9 BROKEN
  latest 1.9
  ! update > "$work/out" 2>&1 && grep -q "NOT restored .*sudoers.d/webcontrol-rs: it does not pass visudo" "$work/out" &&
    grep -q "ROLLBACK INCOMPLETE" "$work/out" && ! grep -q "BAD SUDOERS" "$work/root/etc/sudoers.d/webcontrol-rs" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.0 ] && grep -q "^# installed by 1.0" "$work/root/usr/local/bin/wc-switch"
}

refuses_a_tarball_with_a_link() {
  fresh
  installed 1.0
  publish 1.5
  latest 1.5
  local v="$work/releases/1.5" before
  mkdir "$work/y" && tar -C "$work/y" -xzf "$v/webcontrol-rs-1.5-aarch64.tar.gz" &&
    ln -s /etc "$work/y/webcontrol-rs-1.5-aarch64/etc" &&
    tar -C "$work/y" -czf "$v/webcontrol-rs-1.5-aarch64.tar.gz" webcontrol-rs-1.5-aarch64 && rm -rf "$work/y"
  (cd "$v" && (sha256sum webcontrol-rs-1.5-aarch64.tar.gz 2> /dev/null ||
    shasum -a 256 webcontrol-rs-1.5-aarch64.tar.gz) > SHA256SUMS)
  before="$(snapshot)"
  ! update 2> "$work/err" > /dev/null && grep -q "holds a link" "$work/err" && no_changes_since "$before"
  local result=$?
  rm -rf "$v"
  return "$result"
}

refuses_a_bad_version_flag() {
  fresh
  ! update --version ../../x > /dev/null 2> "$work/err" && grep -q "takes a release tag" "$work/err" &&
    ! grep -qs "^curl" "$work/calls"
}

installed_newer_is_left_alone() {
  fresh
  installed 1.2
  publish 1.1
  latest 1.1
  local before
  before="$(snapshot)"
  update > "$work/out" &&
    grep -qx "Installed 1.2 is newer than the latest published release 1.1; nothing to do. Use --version v1.1 to install it anyway." \
      "$work/out" && no_changes_since "$before" && ! grep -qs "sudo" "$work/calls" &&
    ! grep -qs "aarch64.tar.gz" "$work/calls" || return 1
  update --force > "$work/out" && grep -q "is newer than the latest published release" "$work/out" &&
    no_changes_since "$before" && ! grep -qs "sudo" "$work/calls"
}

numeric_fields_update() {
  fresh
  installed 1.9.0
  publish 1.10.0
  latest 1.10.0
  update > "$work/out" && grep -q "Updated webcontrol-rs to 1.10.0" "$work/out" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.10.0 ]
}

equal_but_spelled_differently_is_up_to_date() {
  fresh
  installed 1.0
  publish 1.0.0
  latest 1.0.0
  update > "$work/out" && grep -q "1.0.0 is installed, the latest published release" "$work/out" &&
    ! grep -qs "sudo" "$work/calls"
}

release_after_its_prerelease_updates() {
  fresh
  installed 2.0.0-rc.1
  publish 2.0.0
  latest 2.0.0
  update > /dev/null && [ "$(version_of "$opt/webcontrol-rs")" = 2.0.0 ] || return 1
  publish 2.0.0-rc.2
  latest 2.0.0-rc.2
  update > "$work/out" && grep -q "Installed 2.0.0 is newer than the latest published release 2.0.0-rc.2" "$work/out" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 2.0.0 ]
}

pinning_an_older_version_warns() {
  fresh
  installed 1.2
  publish 1.1
  latest 1.1
  update --version v1.1 > "$work/out" &&
    grep -qx "Warning: 1.1 is older than the installed 1.2; installing it because --version asked for it." "$work/out" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.1 ]
}

unparseable_installed_version_updates_with_a_warning() {
  fresh
  installed abc1234
  publish 1.1
  latest 1.1
  update > "$work/out" && grep -q 'Warning: cannot compare the installed version "abc1234" with 1.1' "$work/out" &&
    [ "$(version_of "$opt/webcontrol-rs")" = 1.1 ]
}

# status_says <installed> <latest> <wording>
status_says() {
  fresh
  installed "$1"
  publish "$2"
  latest "$2"
  update --status > "$work/out" && grep -qE "^Update: +$3" "$work/out" && ! grep -qs sudo "$work/calls"
}

status_compares_versions() {
  status_says 1.1 1.2 "update available: webcontrol-update installs v1.2$" &&
    status_says 1.2 1.2 "up to date$" &&
    status_says 1.3 1.2 "installed 1.3 is newer than latest published 1.2" &&
    status_says abc1234 1.2 'unknown: cannot compare "abc1234" with 1.2$'
}

# The updater's functions, without running main.
order() { (eval "$(sed '$d' "$rust/webcontrol-update")" && version_order "$1" "$2"); }

orders_versions_like_semver() {
  local a want b got
  while read -r a want b; do
    got="$(order "$a" "$b")"
    [ "$got" = "$want" ] || { echo "  $a against $b should be $want, got $got" >&2; return 1; }
  done <<'EOF'
0.4.0 newer 0.3.0
v0.3.0 older 0.4.0
0.10.0 newer 0.9.0
1.0 same 1.0.0
v1.0.0 same 1.0.0
007.1 same 7.1
1.0.0+build.5 same 1.0.0
99999999999999999999.0 newer 99999999999999999998.0
1.0.0-rc.1 older 1.0.0
1.0.0-alpha older 1.0.0-alpha.1
1.0.0-alpha.1 older 1.0.0-alpha.beta
1.0.0-alpha.beta older 1.0.0-beta
1.0.0-beta.2 older 1.0.0-beta.11
1.0.0-beta.11 older 1.0.0-rc.1
1.0.0-rc.1 same 1.0.0-rc.1
0.3.0-5-gabc1234 newer 0.3.0
0.3.0-5-gabc1234-dirty older 0.3.1
0.3.0-dirty newer 0.3.0
0.3.0-12-gaaa1 newer 0.3.0-5-gbbb2
0.3.0-rc.1-3-gabc1234 older 0.3.0-rc.2
0.3.0-rc.1-3-gabc1234 newer 0.3.0-rc.1
0.3.0-rc.1-dirty older 0.3.0
abc1234 unknown 0.3.0
0.3.0 unknown ../x
EOF
}

help_and_bad_flags() {
  update --help > "$work/out" && grep -q "Only PUBLISHED releases" "$work/out" && grep -q -- "--rollback" "$work/out" &&
    ! update --frobnicate > /dev/null 2>&1
}

check "up to date: exits 0 and changes nothing" up_to_date
check "updates with no earlier backup, backing up every file" update_without_a_backup
check "an update deletes the old backup and makes a new one" update_over_a_backup
check "a first install has nothing to back up" first_install_skips_the_backup
check "a checksum mismatch aborts with nothing changed" checksum_mismatch_changes_nothing
check "refuses a tarball missing an install file" refuses_a_tarball_without_install_files
check "refuses during a job, a pause, a tool change or a probe" refuses_while_busy
check "refuses during a controller firmware upgrade" refuses_during_a_firmware_upgrade
check "a failed health check rolls back by itself" failed_health_check_rolls_back
check "a service answering with another version fails the update" stale_version_rolls_back
check "a controller that does not reconnect fails the update" lost_controller_rolls_back
check "no controller attached is a warning" no_controller_only_warns
check "--rollback restores the backup and keeps it" manual_rollback
check "--rollback refuses during a job or with no backup" rollback_refuses_while_busy_or_without_a_backup
check "--status shows installed, backup and latest" status_shows_versions
check "--status with nothing installed or published" status_without_releases_or_backup
check "--version installs that tag" pins_a_version
check "drafts are invisible: no published release is an error" drafts_are_invisible
check "refuses a machine that is not aarch64" refuses_off_aarch64
check "--repo, WEBCONTROL_REPO and GITHUB_TOKEN" takes_the_repo_and_token
check "--service update: as root, no sudo, progress in the status file" service_update_reports_its_progress
check "--service update reports an automatic rollback" service_reports_an_automatic_rollback
check "--service update reports a rollback that failed" service_reports_a_failed_rollback
check "--service rollback restores the backup" service_rolls_back
check "--service reports a refusal as failed with the reason" service_reports_a_refusal
check "--service update with nothing newer reports up to date" service_up_to_date
check "--service writes quotes, backslashes and tabs as JSON" service_escapes_the_log
check "--service takes the repository and token from webcontrol-rs" service_takes_webcontrol_rs_environment
check "--service needs root and exactly update or rollback" service_needs_root_and_one_action
check "--service refuses while another run holds the lock" service_runs_one_at_a_time
check "a run from a terminal writes no status file" terminal_runs_write_no_status
check "--help and unknown flags" help_and_bad_flags
check "an active service that does not answer counts as busy" hung_service_is_busy
check "a backup that fails keeps the previous one" failed_backup_keeps_the_old_one
check "a job started during the update: files put back, nothing restarted" job_started_during_the_update
check "a restore skips a sudoers rule visudo rejects" restore_skips_a_sudoers_rule_visudo_rejects
check "refuses a tarball holding a link" refuses_a_tarball_with_a_link
check "refuses a --version that is not a tag" refuses_a_bad_version_flag
check "an installed version newer than the latest is left alone, even with --force" installed_newer_is_left_alone
check "versions compare field by field as numbers" numeric_fields_update
check "1.0 and 1.0.0 are the same version" equal_but_spelled_differently_is_up_to_date
check "a release comes after its pre-releases" release_after_its_prerelease_updates
check "--version installs an older release with a warning" pinning_an_older_version_warns
check "an installed version that is not semver updates with a warning" unparseable_installed_version_updates_with_a_warning
check "--status says update available, up to date or newer" status_compares_versions
check "versions order like semver" orders_versions_like_semver

if [ "$failures" != 0 ]; then
  echo "$failures failed"
  exit 1
fi
echo "all webcontrol-update tests passed"
