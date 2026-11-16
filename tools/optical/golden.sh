#!/usr/bin/env bash
# Regenerates wc-optical's fixture frames from the renderer, then Python's goldens for them.
# GOLDEN_PYTHON picks the interpreter; otherwise a scratch venv gets Python's pinned OpenCV and numpy.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
here="$repo/tools/optical"

(cd "$repo/rust" && WC_OPTICAL_WRITE_FIXTURES=1 cargo test -p wc-optical --test render the_fixture_frames)

python="${GOLDEN_PYTHON:-}"
if [[ -z "$python" ]]; then
  venv="${TMPDIR:-/tmp}/wc-optical-golden-venv"
  [[ -x "$venv/bin/python" ]] || uv venv --quiet --python 3.9 "$venv"
  uv pip install --quiet --python "$venv/bin/python" -r "$here/requirements.txt"
  python="$venv/bin/python"
fi
"$python" "$here/golden.py"
