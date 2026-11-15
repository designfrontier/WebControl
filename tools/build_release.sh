#!/usr/bin/env bash
# Builds a PyInstaller release into releases/webcontrol-<version>-<platform>-singledirectory.tar.gz.
# PyInstaller only builds for the machine it runs on, so a Pi release must be built on a Pi or an ARM runner.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: tools/build_release.sh [--skip-frontend] [--skip-firmware] [--singlefile]

  --skip-frontend  use the frontend/dist already present instead of running npm ci && npm run build
  --skip-firmware  do not run buildFirmware.sh (needs git and PlatformIO); bundles firmware/ if it exists
  --singlefile     also build main.spec into webcontrol-<version>-<platform>-singlefile.tar.gz

Environment: PYTHON (default python3) must have requirements.txt and requirements-build.txt installed.
PLATFORM overrides the platform part of the file name (rpi, linux, darwin, win64).
EOF
}

skip_frontend=0
skip_firmware=0
singlefile=0
for arg in "$@"; do
  case "$arg" in
    --skip-frontend) skip_frontend=1 ;;
    --skip-firmware) skip_firmware=1 ;;
    --singlefile) singlefile=1 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "unknown option: $arg" >&2; usage >&2; exit 2 ;;
  esac
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
python="${PYTHON:-python3}"
work_dir="$repo_root/build/release"
out_dir="$repo_root/releases"

# Mirrors how Connection/nonVisibleWidgets.py names the platform, so the self-updater finds the asset.
detect_platform() {
  case "$(uname -s)" in
    Linux)
      local name=""
      # shellcheck source=/dev/null
      [ -r /etc/os-release ] && name="$(. /etc/os-release && echo "${NAME:-}")"
      case "$(echo "$name" | tr '[:upper:]' '[:lower:]')" in
        *raspbian*) echo rpi ;;
        *) echo linux ;;
      esac
      ;;
    Darwin) echo darwin ;;
    MINGW* | MSYS* | CYGWIN*) echo win64 ;;
    *) uname -s | tr '[:upper:]' '[:lower:]' ;;
  esac
}

version="$(sed -n -e 's/^ *pyInstallCurrentVersion = *\([0-9.]*\).*/\1/p' DataStructures/data.py)"
[ -n "$version" ] || { echo "no pyInstallCurrentVersion in DataStructures/data.py" >&2; exit 1; }
platform="${PLATFORM:-$(detect_platform)}"

if ! "$python" -m PyInstaller --version >/dev/null 2>&1; then
  echo "PyInstaller is not installed for $python. Run: $python -m pip install -r requirements-build.txt" >&2
  exit 1
fi

if [ "$skip_frontend" = 1 ]; then
  echo "==> Using the existing frontend/dist"
else
  echo "==> Building the frontend"
  (cd frontend && npm ci && npm run build)
fi
[ -f frontend/dist/index.html ] || { echo "frontend/dist/index.html is missing; build the frontend first" >&2; exit 1; }

if [ "$skip_firmware" = 1 ]; then
  echo "==> Skipping the firmware build"
else
  echo "==> Building the firmware"
  bash ./buildFirmware.sh
fi

pyinstaller_build() {
  rm -rf "$work_dir/dist"
  "$python" -m PyInstaller --noconfirm --clean --distpath "$work_dir/dist" --workpath "$work_dir/work" "$1"
}

mkdir -p "$out_dir"
echo "==> Running PyInstaller (onedir)"
pyinstaller_build main-onedir.spec
onedir="$out_dir/webcontrol-$version-$platform-singledirectory.tar.gz"
tar -C "$work_dir/dist/main" -czf "$onedir" .
echo "Built $onedir"

if [ "$singlefile" = 1 ]; then
  echo "==> Running PyInstaller (onefile)"
  pyinstaller_build main.spec
  onefile="$out_dir/webcontrol-$version-$platform-singlefile.tar.gz"
  tar -C "$work_dir/dist" -czf "$onefile" .
  echo "Built $onefile"
fi
