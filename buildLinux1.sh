#!/usr/bin/env bash
# Builds the frontend, the firmware, and the single-directory and single-file Linux PyInstaller releases into releases/.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec bash "$repo_root/tools/build_release.sh" --singlefile "$@"
