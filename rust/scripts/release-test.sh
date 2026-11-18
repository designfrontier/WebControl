#!/usr/bin/env bash
# Tests release.sh's packaging against a stub binary and a stub firmware pin; nothing is built.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
failures=0

printf ':00000001FF\n' > "$work/fw.hex"
(sha256sum "$work/fw.hex" 2> /dev/null || shasum -a 256 "$work/fw.hex") | cut -d' ' -f1 > "$work/fw.sha256"

# A stand-in for the aarch64 binary that prints the version line it was given and carries the firmware.
stub() {
  printf '#!/bin/sh\n# :00000001FF\necho "%s"\n' "$1" > "$work/webcontrol-rs"
  chmod +x "$work/webcontrol-rs"
}

release() {
  env -u GITHUB_REF_TYPE -u GITHUB_REF_NAME -u GITHUB_OUTPUT -u WC_VERSION \
    WC_SKIP_BUILD=1 WC_BINARY="$work/webcontrol-rs" WC_FIRMWARE_HEX="$work/fw.hex" WC_FIRMWARE_PIN="$work/fw.sha256" \
    "$@" bash "$here/release.sh" "$work/out"
}

check() {
  local name="$1"
  shift
  if "$@"; then echo "ok   $name"; else echo "FAIL $name"; failures=$((failures + 1)); fi
}

reset() { rm -rf "$work/out" "$work/x" "$work/github_output"; }

expected_contents() {
  local top=webcontrol-rs-1.2.3-aarch64
  {
    printf '%s\n' "$top" "$top/rust-checklists"
    printf "$top/%s\n" webcontrol-rs install-pi.sh webcontrol-update wc-switch webcontrol-rs.service \
      webcontrol-conflicts.conf webcontrol-rs.sudoers webcontrol-update@.service README.md
    (cd "$repo/spec" && find rust-checklists -type f) | sed "s|^|$top/|"
  } | sort
}

packages_the_install_files() {
  reset
  stub "webcontrol-rs 1.2.3 (abc1234, contract 1)"
  release WC_VERSION=1.2.3 > /dev/null &&
    diff <(tar -tzf "$work/out/webcontrol-rs-1.2.3-aarch64.tar.gz" | sed 's|/$||' | sort) <(expected_contents) &&
    mkdir "$work/x" && tar -C "$work/x" -xzf "$work/out/webcontrol-rs-1.2.3-aarch64.tar.gz" &&
    [ -x "$work/x/webcontrol-rs-1.2.3-aarch64/webcontrol-rs" ] &&
    cmp -s "$work/x/webcontrol-rs-1.2.3-aarch64/webcontrol-rs" "$work/webcontrol-rs" &&
    cmp -s "$work/x/webcontrol-rs-1.2.3-aarch64/install-pi.sh" "$repo/rust/install-pi.sh" &&
    [ -x "$work/x/webcontrol-rs-1.2.3-aarch64/webcontrol-update" ] &&
    cmp -s "$work/x/webcontrol-rs-1.2.3-aarch64/webcontrol-update" "$repo/rust/webcontrol-update" &&
    cmp -s "$work/x/webcontrol-rs-1.2.3-aarch64/webcontrol-rs.service" "$repo/rust/deploy/webcontrol-rs.service" &&
    cmp -s "$work/x/webcontrol-rs-1.2.3-aarch64/README.md" "$repo/rust/README.md"
}

writes_a_checksum_for_every_asset() {
  reset
  stub "webcontrol-rs 1.2.3 (abc1234, contract 1)"
  release WC_VERSION=1.2.3 > /dev/null &&
    [ "$(find "$work/out" -type f -exec basename {} \; | sort | tr '\n' ' ')" = \
      "SHA256SUMS webcontrol-rs-1.2.3-aarch64 webcontrol-rs-1.2.3-aarch64.sha256 webcontrol-rs-1.2.3-aarch64.tar.gz webcontrol-rs-1.2.3-aarch64.tar.gz.sha256 " ] &&
    (cd "$work/out" && shasum -a 256 -c SHA256SUMS > /dev/null && shasum -a 256 -c ./*.sha256 > /dev/null) &&
    [ "$(wc -l < "$work/out/SHA256SUMS" | tr -d ' ')" = 2 ] &&
    cmp -s "$work/out/webcontrol-rs-1.2.3-aarch64" "$work/webcontrol-rs"
}

takes_the_version_from_the_tag() {
  reset
  stub "webcontrol-rs 2.0.0-rc.1 (abc1234, contract 1)"
  release GITHUB_REF_TYPE=tag GITHUB_REF_NAME=v2.0.0-rc.1 GITHUB_OUTPUT="$work/github_output" > /dev/null &&
    [ -f "$work/out/webcontrol-rs-2.0.0-rc.1-aarch64.tar.gz" ] &&
    grep -qx "version=2.0.0-rc.1" "$work/github_output"
}

takes_the_version_from_git_describe_otherwise() {
  reset
  local described
  described="$(git -C "$repo" describe --tags --match 'v[0-9]*' --always --dirty | sed 's/^v//')"
  stub "webcontrol-rs $described (abc1234, contract 1)"
  release > /dev/null && [ -f "$work/out/webcontrol-rs-$described-aarch64.tar.gz" ]
}

refuses_a_binary_reporting_another_version() {
  reset
  stub "webcontrol-rs 0.1.0 (abc1234, contract 1)"
  ! release WC_VERSION=1.2.3 2> "$work/err" && grep -q "webcontrol-rs 1.2.3 (" "$work/err" &&
    [ ! -e "$work/out/webcontrol-rs-1.2.3-aarch64.tar.gz" ]
}

checks_a_binary_it_cannot_run_by_its_version_literal() {
  reset
  printf '\177ELF\0webcontrol-rs 1.2.3 (abc1234\0:00000001FF\n' > "$work/webcontrol-rs"
  chmod 644 "$work/webcontrol-rs"
  release WC_VERSION=1.2.3 > /dev/null || return 1
  printf '\177ELF\0webcontrol-rs 0.1.0 (abc1234\0:00000001FF\n' > "$work/webcontrol-rs"
  ! release WC_VERSION=1.2.3 2> /dev/null
}

refuses_firmware_off_its_pin() {
  reset
  stub "webcontrol-rs 1.2.3 (abc1234, contract 1)"
  printf ':00000001FE\n' > "$work/other.hex"
  ! release WC_VERSION=1.2.3 WC_FIRMWARE_HEX="$work/other.hex" 2> "$work/err" &&
    grep -q "SHA-256" "$work/err" && [ ! -e "$work/out/webcontrol-rs-1.2.3-aarch64.tar.gz" ]
}

refuses_a_binary_without_the_firmware() {
  reset
  printf '#!/bin/sh\necho "webcontrol-rs 1.2.3 (abc1234, contract 1)"\n' > "$work/webcontrol-rs"
  ! release WC_VERSION=1.2.3 2> "$work/err" && grep -q "does not embed" "$work/err"
}

archives_files_as_root() {
  reset
  stub "webcontrol-rs 1.2.3 (abc1234, contract 1)"
  release WC_VERSION=1.2.3 > /dev/null &&
    ! tar -tvzf "$work/out/webcontrol-rs-1.2.3-aarch64.tar.gz" | grep -Ev " root(/| +)root " | grep -q .
}

refuses_without_firmware() {
  reset
  stub "webcontrol-rs 1.2.3 (abc1234, contract 1)"
  ! release WC_VERSION=1.2.3 WC_FIRMWARE_HEX= 2> "$work/err" && grep -q "WC_FIRMWARE_HEX" "$work/err"
}

the_real_pin_is_the_default() {
  reset
  stub "webcontrol-rs 1.2.3 (abc1234, contract 1)"
  ! env -u WC_FIRMWARE_PIN WC_SKIP_BUILD=1 WC_BINARY="$work/webcontrol-rs" WC_FIRMWARE_HEX="$work/fw.hex" \
    WC_VERSION=1.2.3 bash "$here/release.sh" "$work/out" 2> "$work/err" &&
    grep -q "$(cat "$repo/rust/firmware/holey-51.292.sha256")" "$work/err"
}

refuses_a_version_unfit_for_a_file_name() {
  reset
  stub "webcontrol-rs ../x (abc1234, contract 1)"
  ! release WC_VERSION=../x 2> "$work/err" && grep -q "not a release version" "$work/err"
}

check "packages the binary, install files, README and checklists" packages_the_install_files
check "writes a SHA-256 for every asset" writes_a_checksum_for_every_asset
check "takes the version from a v* tag and reports it to GitHub" takes_the_version_from_the_tag
check "takes the version from git describe without a tag" takes_the_version_from_git_describe_otherwise
check "refuses a binary whose --version is another release" refuses_a_binary_reporting_another_version
check "checks a binary it cannot run by the version literal in it" checks_a_binary_it_cannot_run_by_its_version_literal
check "refuses a firmware hex off its pinned SHA-256" refuses_firmware_off_its_pin
check "refuses to package without the firmware hex" refuses_without_firmware
check "refuses a binary that does not embed the firmware" refuses_a_binary_without_the_firmware
check "archives every file as root, not the build user" archives_files_as_root
check "checks the hex against rust/firmware/holey-51.292.sha256 by default" the_real_pin_is_the_default
check "refuses a version that is not fit for a file name" refuses_a_version_unfit_for_a_file_name

if [ "$failures" != 0 ]; then
  echo "$failures failed"
  exit 1
fi
echo "all release script tests passed"
