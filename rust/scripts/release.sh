#!/usr/bin/env bash
# Builds and packages the install assets, as .github/workflows/release.yml does:
#   rust/scripts/release.sh [out-dir]    (default rust/target/release-assets)
# writes webcontrol-rs-<version>-<arch>.tar.gz and the bare binary for aarch64 (the Pi) and x86_64, a .sha256 beside
# each and one SHA256SUMS.
# WC_FIRMWARE_HEX names the 51.292 hex (required); WC_VERSION overrides the version from the tag or git describe.
set -euo pipefail

rust="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(dirname "$rust")"
arches=(aarch64 x86_64)
targets="${CARGO_TARGET_DIR:-$rust/target}"
pin="${WC_FIRMWARE_PIN:-$rust/firmware/holey-51.292.sha256}"
out="${1:-$rust/target/release-assets}"

fail() {
  echo "$*" >&2
  exit 1
}

sha256() { (sha256sum "$1" 2> /dev/null || shasum -a 256 "$1") | cut -d' ' -f1; }

release_version() {
  [ -n "${WC_VERSION:-}" ] && { echo "$WC_VERSION"; return; }
  [ "${GITHUB_REF_TYPE:-}" = tag ] && { echo "${GITHUB_REF_NAME#v}"; return; }
  git -C "$repo" describe --tags --match 'v[0-9]*' --always --dirty | sed 's/^v//'
}

check_firmware() {
  local hex="${WC_FIRMWARE_HEX:-}"
  [ -n "$hex" ] && [ -f "$hex" ] ||
    fail "WC_FIRMWARE_HEX must name the 51.292 hex (bash rust/firmware/docker-build.sh <dir> builds it)"
  local actual pinned
  actual="$(sha256 "$hex")"
  pinned="$(tr -d '[:space:]' < "$pin")"
  [ "$actual" = "$pinned" ] || fail "$hex has SHA-256 $actual, but $pin pins $pinned"
}

build() {
  [ "${WC_SKIP_BUILD:-0}" = 1 ] && return 0
  (cd "$repo/frontend" && npm ci && npm run build)
  command -v zig > /dev/null || python3 -c "import ziglang" 2> /dev/null ||
    fail "cargo zigbuild needs zig: pip install ziglang (or put zig on PATH)"
  local arch
  for arch in "${arches[@]}"; do
    (cd "$rust" && WC_VERSION="$version" WC_GIT_SHA="$(git -C "$repo" rev-parse --short HEAD)" \
      cargo zigbuild --release --target "$arch-unknown-linux-gnu.2.36" -p wc-server)
  done
}

binary_for() { echo "$targets/$1-unknown-linux-gnu/release/webcontrol-rs"; }

check_firmware_embedded() {
  local binary="$1"
  LC_ALL=C grep -aqF "$(head -n 1 "$WC_FIRMWARE_HEX")" "$binary" || fail "$binary does not embed $WC_FIRMWARE_HEX"
}

# Runs the binary where it can (its own architecture); elsewhere looks for the version literal it carries.
check_version() {
  local binary="$1" expected="webcontrol-rs $version (" said
  if said="$("$binary" --version 2> /dev/null)"; then
    [[ "$said" == "$expected"* ]] || fail "$binary --version says \"$said\", not \"$expected...\""
  else
    LC_ALL=C grep -aqF "$expected" "$binary" || fail "$binary does not carry \"$expected\""
  fi
}

package() {
  local binary="$1" name="webcontrol-rs-$version-$2"
  mkdir -p "$stage/$name"
  install -m 755 "$binary" "$stage/$name/webcontrol-rs"
  install -m 755 "$rust/install-pi.sh" "$rust/webcontrol-update" "$rust/deploy/wc-switch" "$stage/$name/"
  install -m 644 "$rust/deploy/webcontrol-rs.service" "$rust/deploy/webcontrol-update@.service" \
    "$rust/deploy/webcontrol-conflicts.conf" "$rust/deploy/webcontrol-rs.sudoers" "$rust/README.md" "$stage/$name/"
  cp -R "$repo/spec/rust-checklists" "$stage/$name/rust-checklists"
  local as_root=(--uid 0 --gid 0 --uname root --gname root --no-xattrs --no-mac-metadata)
  tar --version | grep -q GNU && as_root=(--owner=root:0 --group=root:0)
  COPYFILE_DISABLE=1 tar "${as_root[@]}" -C "$stage" -czf "$out/$name.tar.gz" "$name"
  install -m 755 "$binary" "$out/$name"
  local asset
  for asset in "$name.tar.gz" "$name"; do
    echo "$(sha256 "$out/$asset")  $asset" | tee "$out/$asset.sha256" >> "$out/SHA256SUMS"
  done
}

version="$(release_version)"
[[ "$version" =~ ^[0-9A-Za-z][0-9A-Za-z.+_-]*$ ]] || fail "\"$version\" is not a release version"
check_firmware
build
for arch in "${arches[@]}"; do
  check_version "$(binary_for "$arch")"
  check_firmware_embedded "$(binary_for "$arch")"
done
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$out"
rm -f "$out"/webcontrol-rs-* "$out/SHA256SUMS"
for arch in "${arches[@]}"; do package "$(binary_for "$arch")" "$arch"; done
[ -n "${GITHUB_OUTPUT:-}" ] && echo "version=$version" >> "$GITHUB_OUTPUT"
echo "webcontrol-rs $version assets in $out:"
ls -1 "$out"
