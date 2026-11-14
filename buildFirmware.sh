#!/usr/bin/env bash
# Builds the bundled controller firmware into firmware/<variant>/<variant>-<version>.hex, the layout
# Actions/firmware.py (FIRMWARE_FOLDERS) reads. Needs git and PlatformIO's pio on PATH.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
firmware_dir="${FIRMWARE_DIR:-$repo_root/firmware}"
build_dir="${BUILD_DIR:-$repo_root/build/firmware}"

variants=(madgrizzle maslowcnc holey)

source_of() {
  case "$1" in
    madgrizzle) echo "https://github.com/madgrizzle/Firmware.git bf4350ffd9bc154832505fc0125abd2c4c04dba7" ;;
    maslowcnc) echo "https://github.com/MaslowCNC/Firmware.git e1e0d020fff1f4f7c6b403a26a85a16546b7e15b" ;;
    holey) echo "https://github.com/madgrizzle/Firmware.git 950fb23396171cbd456c2d4149455cc45f5e6bc3" ;;
    *) echo "unknown firmware variant: $1" >&2; return 1 ;;
  esac
}

build_variant() {
  local variant=$1 repo sha src version hex
  read -r repo sha <<<"$(source_of "$variant")"
  src="$build_dir/$variant"
  git clone --quiet "$repo" "$src"
  git -C "$src" checkout --quiet "$sha"
  # The holey build never installed Servo; keep each variant's pinned build unchanged.
  if [ "$variant" != holey ]; then
    (cd "$src" && pio lib install "Servo")
  fi
  (cd "$src" && pio run -e megaatmega2560)
  version="$(sed -n -e 's/^.*VERSIONNUMBER //p' "$src/cnc_ctrl_v1/Maslow.h" | tr -d '[:space:]')"
  if [ -z "$version" ]; then
    echo "no VERSIONNUMBER in $variant's cnc_ctrl_v1/Maslow.h" >&2
    return 1
  fi
  hex="$src/.pio/build/megaatmega2560/firmware.hex"
  [ -f "$hex" ] || hex="$src/.pioenvs/megaatmega2560/firmware.hex"
  mkdir -p "$firmware_dir/$variant"
  cp "$hex" "$firmware_dir/$variant/$variant-$version.hex"
}

rm -rf "$build_dir" "$firmware_dir"
mkdir -p "$build_dir" "$firmware_dir"
for variant in "${variants[@]}"; do
  build_variant "$variant"
done

missing=0
for variant in "${variants[@]}"; do
  if ! compgen -G "$firmware_dir/$variant/$variant-*.hex" >/dev/null; then
    echo "missing firmware/$variant/$variant-<version>.hex" >&2
    missing=1
  fi
done
exit "$missing"
