#!/usr/bin/env bash
# Runs build.sh in the arm64 Ubuntu image the pinned digest was made in; the compiler's output differs between hosts,
# so CI builds on an arm64 runner too. Usage: docker-build.sh <out-dir>; writes <out-dir>/holey-51.292.hex.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out="$(mkdir -p "${1:?usage: docker-build.sh <out-dir>}" && cd "$1" && pwd)"
image=ubuntu:24.04@sha256:2198e4c65076e9cb0b3d695219df667c0e3b0072da64ca3c2648aca17dcecaf5

docker run --rm --platform linux/arm64 -v "$here:/fw:ro" -v "$out:/out" "$image" bash -c '
  set -euo pipefail
  apt-get update -qq >/dev/null
  apt-get install -y -qq git python3 python3-venv ca-certificates >/dev/null
  bash /fw/build.sh /out/holey-51.292.hex'
