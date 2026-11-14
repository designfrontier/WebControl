#!/usr/bin/env bash
# Builds the firmware, then the single-directory and single-file Linux PyInstaller releases into releases/.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$repo_root"
bash ./buildFirmware.sh
mkdir -p releases

rm -rf dist
pyinstaller main-onedir.spec
(cd dist/main && tar -zcvf ../../releases/webcontrol-linux-singledirectory.tar.gz .)

rm -rf dist
pyinstaller main.spec
(cd dist && tar -zcvf ../releases/webcontrol-linux-singlefile.tar.gz .)
