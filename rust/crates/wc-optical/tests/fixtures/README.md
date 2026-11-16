# wc-optical fixtures

## Regenerating

From the repo root, after changing the renderer or `cases()` in `tests/render.rs`:

```sh
tools/optical/golden.sh
```

It rewrites `frames/*.png` and `frames.json` (`WC_OPTICAL_WRITE_FIXTURES=1 cargo test -p wc-optical --test render
the_fixture_frames`), then runs `tools/optical/golden.py`. That script loads `Actions/opticalCalibration.py` from
`git show 220aa0e:…` into a temp dir, stubs `DataStructures/makesmithInitFuncs.py`, turns `time.sleep` off and feeds
each frame 10 times to `processImage` through a fake camera. It needs Python's pins, `opencv-python==4.6.0.66` and
`numpy==1.23.5` (`tools/optical/requirements.txt`). Set `GOLDEN_PYTHON` to an interpreter that has them, for example
the old repo `.venv`; without it, the script builds a scratch venv with `uv` under `$TMPDIR`. The script only needs
git history, so it works from a clean checkout.

## Files

- `frames/<name>.png`: 8-bit grey frames from `render::render`, 320 x 240 at 4 px/mm, 0.6" squares at 3" pitch.
  `the_fixture_frames_are_what_the_renderer_draws` fails if the renderer no longer draws them byte for byte.
- `frames.json`: each frame's camera position, `Scene` and target, and its ground truth. `truth.offsetMm` is the
  nearest square's centre minus the camera position, in machine mm (+Y up); `truth.centrePx` is where `Scene::to_pixel`
  puts that centre, in OpenCV pixel terms (pixel `n`'s centre is at `n`, so the optical centre of a 320-wide frame is
  160). With radial distortion the drawn square is no longer symmetric, so its centroid sits slightly outward of
  `centrePx`. `truth` is `null` when no square is in view (`off_grid`).
- `python_golden.json`: what Python computed for every frame, plus its CSV text for three seeded grids.

## Raw and shimmed rows

Each frame has three Python results. All are informative, not a gate: the acceptance bar is the ground truth
(within 0.3 px, spec decision 6).

- `findCentre`: find-centre mode. It does not rotate the centre, so OpenCV's angle convention does not affect it.
  Rust's find-centre should be within 0.5 px of `centrePx`.
- `calibrateRaw`: calibration mode on OpenCV 4.6.0 as pinned. `minAreaRect` reports angles in (0, 90] there, not
  OpenCV 3's [-90, 0), and Python's `_angle` logic was written for OpenCV 3, so an axis-aligned square is rotated a
  quarter turn: `sharp_offset` gives (2.54, -3.81) mm for a true (3.81, 2.54). These rows record the bug
  (`spec/optical-calibration.md` §1.9.1); Rust must not match them.
- `calibrateShim`: calibration mode with `minAreaRect`'s angle mapped back into OpenCV 3's range. This is what Python
  meant, and the row Rust is compared with (within 0.5 px / 0.05 mm). Where the raw and shimmed rows agree (a
  positive tilt, which both conventions report the same way), the bug did not bite.

Python's own numbers are not the truth either: it takes the centre from `boxPoints` rounded to integers, so on a
sharp frame it is often 0.25 to 0.5 px off.

The `csv` entries give each grid as the wire's `[column][row]` (`null` = not measured, which Python wrote as 0) and
the exact file `saveCalibrationToCSV` wrote. `awkward` holds values that test rounding: -0.001 and -0.0 print as
`-0.00`, 2.675 as `2.67`.

The PNGs are pinned on macOS; the renderer uses `sin`, `cos`, `exp` and `ln`, whose last bit may differ between
platforms' maths libraries, which could in principle flip one pixel by one level on another platform.
