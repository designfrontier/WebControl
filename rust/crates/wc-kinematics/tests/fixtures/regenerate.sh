#!/usr/bin/env bash
# Rebuilds firmware32.json from firmware32.c, the firmware's float32 triangular inverse.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cc -std=c99 -O0 -ffp-contract=off -o "$work/firmware32" "$here/firmware32.c" -lm
"$work/firmware32" >"$here/firmware32.json"
echo "$here/firmware32.json"
