#!/usr/bin/env bash
# Builds webcontrol-rs for the Pi and installs or updates it there, from the Mac.
#   rust/deploy.sh build                 frontend/dist + the aarch64 binary (glibc 2.36: bookworm and trixie)
#   rust/deploy.sh install  [host]       first time: binary, systemd unit, drop-in, wc-switch and webcontrol-update; starts it
#   rust/deploy.sh push     [host]       new binary; restarts webcontrol-rs only if it is running
#   rust/deploy.sh rollback [host]       swap the previous binary back
# WC_ARCH=x86_64 builds for, and deploys to, an x86_64 Linux laptop instead.
set -euo pipefail

rust="$(cd "$(dirname "$0")" && pwd)"
repo="$(dirname "$rust")"
target="${WC_ARCH:-aarch64}-unknown-linux-gnu"
binary="${WC_BINARY:-${CARGO_TARGET_DIR:-$rust/target}/$target/release/webcontrol-rs}"
command="${1:-}"
host="${2:-pi@maslow.local}"

# The 51.292 image the upgrade flashes, built from its tag in Docker unless WC_FIRMWARE_HEX names one.
firmware() {
  [ -n "${WC_FIRMWARE_HEX:-}" ] && return 0
  local out="$rust/target/firmware"
  if [ ! -f "$out/holey-51.292.hex" ]; then
    if ! command -v docker > /dev/null; then
      echo "no docker: building without the 51.292 image, so the upgrade wizard will refuse" >&2
      return 0
    fi
    bash "$rust/firmware/docker-build.sh" "$out"
  fi
  export WC_FIRMWARE_HEX="$out/holey-51.292.hex"
}

build() {
  [ "${WC_SKIP_BUILD:-0}" = 1 ] && return 0
  firmware
  (cd "$repo/frontend" && npm ci && npm run build)
  if ! command -v zig > /dev/null && ! python3 -c "import ziglang" 2> /dev/null; then
    echo "cargo zigbuild needs zig: pip install ziglang (or put zig on PATH)" >&2
    exit 1
  fi
  (cd "$rust" && WC_GIT_SHA="$(git -C "$repo" rev-parse --short HEAD)" \
    cargo zigbuild --release --target "$target.2.36" -p wc-server)
  echo "built $binary"
}

# Asks the Pi itself, so it works whatever the Mac can reach; an unreachable backend runs no job.
refuse_during_a_job() {
  if ssh "$host" "curl -fsS --max-time 2 http://localhost:5000/api/state" 2> /dev/null |
    grep -Eq '"jobInProgress": *true'; then
    echo "A job is running or paused on $host; stop it first." >&2
    exit 1
  fi
}

case "$command" in
  build) build ;;
  install)
    build
    stage=/tmp/webcontrol-rs-install
    # shellcheck disable=SC2029 # $stage is meant to expand here.
    ssh "$host" "rm -rf $stage && mkdir -p $stage"
    scp "$binary" "$rust/install-pi.sh" "$rust/webcontrol-update" "$rust/deploy/webcontrol-rs.service" \
      "$rust/deploy/webcontrol-conflicts.conf" "$rust/deploy/wc-switch" "$rust/deploy/webcontrol-rs.sudoers" \
      "$rust/deploy/webcontrol-update@.service" "$host:$stage/"
    ssh -t "$host" "bash $stage/install-pi.sh $stage"
    ;;
  push)
    refuse_during_a_job
    build
    scp "$binary" "$host:/tmp/webcontrol-rs.new"
    ssh -t "$host" "set -e; cd /opt/webcontrol-rs
      sudo install -m 755 /tmp/webcontrol-rs.new webcontrol-rs.new && rm /tmp/webcontrol-rs.new
      if [ -f webcontrol-rs ]; then sudo mv webcontrol-rs webcontrol-rs.prev; fi
      sudo mv webcontrol-rs.new webcontrol-rs
      ./webcontrol-rs --version
      sudo systemctl try-restart webcontrol-rs"
    echo "pushed to $host; restarted only if webcontrol-rs was running"
    ;;
  rollback)
    refuse_during_a_job
    ssh -t "$host" "set -e; cd /opt/webcontrol-rs; test -f webcontrol-rs.prev
      sudo mv webcontrol-rs webcontrol-rs.swap && sudo mv webcontrol-rs.prev webcontrol-rs
      sudo mv webcontrol-rs.swap webcontrol-rs.prev
      ./webcontrol-rs --version
      sudo systemctl try-restart webcontrol-rs"
    ;;
  *)
    sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
    ;;
esac
