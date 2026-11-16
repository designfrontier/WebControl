#!/usr/bin/env bash
# Runs the workspace tests under cargo llvm-cov and fails if any crate's line coverage is below the floor.
set -euo pipefail

FLOOR="${COVERAGE_FLOOR:-95}"
# Camera adapters that only real hardware exercises (spec/optical-calibration.md, decision 5).
EXCLUDED='/crates/wc-optical/src/camera/(v4l2|rpicam)[.]rs$'
cd "$(dirname "$0")/.."
report="$(mktemp)"
trap 'rm -f "$report"' EXIT

cargo llvm-cov --workspace --json --summary-only --ignore-filename-regex "$EXCLUDED" --output-path "$report" "$@"

jq -r --argjson floor "$FLOOR" '
  [.data[0].files[]
    | select(.filename | test("/crates/[^/]+/src/"))
    | {crate: (.filename | capture("/crates/(?<c>[^/]+)/src/").c), count: .summary.lines.count, covered: .summary.lines.covered}]
  | group_by(.crate)
  | map({crate: .[0].crate, count: (map(.count) | add), covered: (map(.covered) | add)})
  | map(. + {percent: (if .count == 0 then 100 else (.covered * 10000 / .count | floor) / 100 end)})
  | (map("\(.crate)\t\(.covered)/\(.count)\t\(.percent)%") | join("\n")),
    (map(select(.percent < $floor)) | if length == 0 then "every crate is at or above \($floor)% lines"
      else "BELOW \($floor)%: \(map(.crate) | join(", "))" end)
' "$report" | tee /dev/stderr | tail -1 | grep -q "^every crate"
