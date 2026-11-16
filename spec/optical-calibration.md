# Optical calibration (experimental): what Python did and the Rust/React plan

Status: plan, 2026-11-15. Supersedes the owner decision "Camera and optical calibration dropped"
(`spec/RUST-PLAN.md:30-31`) once the owner accepts it; M7 in `spec/RUST-PLAN.md:437` is the old one-line version.

Python citations are `file:line` at commit `220aa0e` (`git show 220aa0e:<file>`). The React screen is at `59895dd^`
(`git show 59895dd^:frontend/src/features/calibration/OpticalCalibration.tsx`); `59895dd` deleted it. Rust
citations are on `master` (`f6a976e`).

**Summary.** The Python feature was a closed loop: a camera on the sled looks at a printed grid of 0.6" squares at
3" pitch, the machine moves the sled until each square is under the camera's centre, and the extra offset each square
needed is that point's positioning error. It produced a 31 x 15 error grid and a quadratic surface fit. Both were for
the "custom" (madgrizzle) firmware, which was to correct targets inside its inverse kinematics. **Nothing consumes that
result today.** The owner runs holey firmware, which has no correction. The one firmware that had a correction
(madgrizzle 101.25) reads different `$` keys from the ones WebControl sends (§1.8). And Python's calibration-mode image
maths gives wrong answers under the OpenCV it pinned (§1.9). So this plan ports the measurement, fixes it, and lets
the owner choose what the measurement feeds (§5, Q1). It recommends a holey geometry fit, which works with the
firmware the owner already runs.

---

## 1. What the Python feature did

### 1.1 Parts

| Part | Where |
|---|---|
| The whole algorithm: settings, motion, image pipeline, fits, CSV, save | `Actions/opticalCalibration.py` (727 lines) |
| Dispatch of the 12 optical actions | `Actions/actions.py:256-299`; `adjustCenter` `:1338-1352`; `calibrationCurve` / `calibrationError` setting requests `:1369-1398`; camera `toggleCamera` / `cameraStatus` / `queryCamera` `:1815-1862` |
| Camera capture thread (OpenCV `VideoCapture(0)`) | `Background/webcamVideoStream.py` |
| `[Measure` reply → callback | `Background/messageProcessor.py:79-86` (`data.measureRequest(measuredDist)`) |
| Image pushes to the UI | `Background/UIProcessor.py:69-91` (`OpticalCalibrationImageUpdated`, `cameraImageUpdated`, `OpticalCalibrationTestImageUpdated`) |
| Error-array parse and `$O` send; optical `$` keys | `config/config.py:26-28` (`UNSENDABLE_FIRMWARE_KEYS = (85, 86, 144)`, `OPTICAL_FIRMWARE_KEYS = 87..98`), `:491-517` (sync branch), `:572-627` (`parseErrorArray`), `:629-656` (`sendErrorArray`), `:705` (restore skips optical keys below 100), `:843-846` (camera setting changes) |
| Settings | `defaultwebcontrol.json:791` "Optical Calibration Settings" (`xyErrorArray` $85, `calX0..5` $87-92, `calY0..5` $93-98, then 16 UI-only keys); `:221` `enableOpticalCalibration` $144 (Advanced); `:1054` "Camera Settings" (`videoSize`, `fps`, `cameraSleep`) |
| New-UI API | `api/calibration.py:18-37` (defaults, ints, the three camera actions), `:101-123` (`optical_inputs`, `optical_matrices`), `:216-232` (`GET /api/calibration/optical`, `PUT /api/calibration/optical/settings`), `:235-250` (`opticalCalibration` socket event) |
| Classic UI | `templates/opticalCalibration.html` (473 lines), `static/scripts/opticalCalibration.js` (486), menu button gated on custom firmware `templates/actions.html:40` |
| Tests | `tests/test_optical_calibration.py` (synthetic 320x240 frames: one black square; FakeCamera), `tests/test_webcam.py` |
| Help pages | None; `docs/index.md:117-120` only credits OpenCV/numpy/scipy/imutils |

### 1.2 Settings (all under "Optical Calibration Settings" unless noted)

| Key | Default | Meaning |
|---|---|---|
| `markerX`, `markerY` | 0.6 | Printed square size, **inches**; gives px per mm |
| `scaleX`, `scaleY` | 1.0 | Printer scale error of the target sheet |
| `opticalCenterX`, `opticalCenterY` | 0 | Pixel the sled reference is at; 0 means image centre (`opticalCalibration.py:245-249`) |
| `positionTolerance` | 0.125 | mm; stop re-homing a square when both offsets are below it |
| `tlX`, `tlY`, `brX`, `brY` | -15, 7, 15, -7 | Scan extents in grid cells (+X right, +Y up) |
| `calibrationExtents` | "Full Area" | UI preset name (Full Area, the four 48x36 quarters, Custom) |
| `autoScanDirection` | 0 | 0 horizontal serpentine, 1 vertical |
| `gaussianBlurValue` | 5 | Kernel size (odd) |
| `cannyLowValue`, `cannyHighValue` | 50, 100 | Canny thresholds |
| `xyErrorArray` ($85) | 930 zeros | Error grid, ints of mm x 1000, X then Y, x fastest |
| `calX0..5`, `calY0..5` ($87-98) | "0" | Surface-fit coefficients, stored as strings |
| Advanced `enableOpticalCalibration` ($144) | 0 | "Use Calibration Error Matrix (Experimental)" |
| Camera `videoSize`, `fps`, `cameraSleep` | 640x480, 5, 0.001 | Capture thread knobs |

### 1.3 Grid and coordinates

- 31 columns x 15 rows of squares, 3" (76.2 mm) apart, centred on the sheet: column `i` in -15..15, row `j` in -7..7,
  nominal centre `(i*76.2, j*76.2)` mm. The grid spans 90" x 42" (2286 x 1067 mm) on a 96" x 48" sheet.
- Storage index `[i+15][7-j]` (`opticalCalibration.py:365-369`): column-major, row 0 at the top.
- `xyErrorArray` string: for X then Y, for row 0..14, for column 0..30, `int(err*1000)` comma-joined
  (`opticalCalibration.py:571-601`). `parseErrorArray` reads it back, x fastest (`config.py:572-627`). Note
  `int()` truncates toward zero (−0.0019 mm → 0, 1.9999 → 1).

### 1.4 Procedure and motion (`on_Calibrate` → `on_AutoHome` → `HomeIn` → `on_CenterOnSquare`)

1. **Start** (`on_Calibrate`, `:461-482`): takes the settings object from the UI, **saves it to webcontrol.json**
   (`saveOpticalCalibrationConfiguration`, `:151-175`), starts the camera, calls `on_AutoHome(False)`.
2. **First square** (`on_AutoHome`, `:383-400`): `HomingX = HomingY = 0`, starts at `(tlX, tlY)`, remembers the
   units (`oldUnits`), sets `inAutoMode`.
3. **Move** (`HomeIn`, `:188-206`): target in inches is `pos*3 + Homing/25.4` (rounded to 4 places). If the machine is
   in mm it switches the **UI setting** to inches (`updateSetting("toInches")`, which broadcasts to every client), then
   queues `G20`, `G90`, `G0 X.. Y..`, **`G91`**, sets `measureRequest = on_CenterOnSquare` and queues `B10 L`. The
   `[Measure: …]` reply to B10 arrives after the G0 has finished, so it acts as a "move done" barrier. The chain length
   it carries is ignored.
4. **Look** (`processImage`, `:209-334`), called from the serial message thread: sleep 1 s (settle), then up to 10 good
   frames (§1.5), each giving the square's offset `(Dx, Dy)` in mm from the optical centre. The mean comes after a
   ±2σ outlier cut (`removeOutliersAndAverage`, `:110-116`). If 10 frames fail (no frame, or no contour > 1000 px²),
   it gives up and `on_CenterOnSquare` returns False. **The scan then just stops there**: no alert, no state reset,
   and `inAutoMode` stays True.
5. **Re-home or record** (`on_CenterOnSquare`, `:336-381`): `HomingX += Dx`, `HomingY += Dy`. If either `|D|` ≥
   `positionTolerance`, move again (step 3) with the corrected target. **There is no limit on attempts and none on
   step size.** Otherwise it records `err = Homing + pos*76.2*(1 - scale)` per axis into `calErrors[i+15][7-j]`.
   Then it advances (auto mode) or stops the camera (single shot).
6. **Advance** (`on_AutoHome`, `:401-457`): serpentine. Horizontal flips direction at each row end and steps `Y`
   down. Vertical does the same by columns. `HomingX/Y` carry over as the next square's first guess, with
   `HomingY -= 7.0` mm ("drop down 7 mm"). At the end it restores units, stops the camera and (horizontal only, `:428-432`)
   pushes `updateOpticalCalibrationError {errorX, errorY}`.
7. **Stop** (`stopOpticalCalibration`, `:81-92`): `!` via the quick queue, clears the gcode queue, clears the modes,
   restores units. It sends no `~`.

### 1.5 Image pipeline (`processImage`, `:209-334`) — what Rust must reproduce

Per frame: BGR→gray; `GaussianBlur(k, k, σ=0)`; `Canny(low, high)`; dilate then erode 3x3 once each; external
contours (`RETR_EXTERNAL`, `CHAIN_APPROX_SIMPLE`); the largest by area, and only if its area > 1000 px²;
`simplifyContour` (`:118-133`: `approxPolyDP` with ε = t·perimeter, t from 0.01 rising by 0.01 until ≤ 4 vertices,
or ≤ 20 in find-centre mode, then backing off if it went under); `minAreaRect` → angle; `boxPoints` → **int**
corners, ordered tl, tr, br, bl (`orderPoints`, `:696-708`). Then:

- centre `(xB, yB)` = mean of the 4 int corners;
- px per mm: `xD` = distance between the left and right edge midpoints / (markerX·25.4), and `yD` the same for top
  and bottom;
- rotation: `_angle = angle+90 if |angle+90| < 30 else angle`; `if _angle < 30: _angle = -_angle`. In calibration
  mode (not find-centre) the centre is rotated about the optical centre by `_angle` (`translatePoint`, `:177-186`);
- `Dx = (xB - xA)/xD`, `Dy = (yA - yB)/yD` (image y points down, machine +Y up), `Dist = hypot`.

Find-centre mode (`findCenter`, `:528-539`) returns the averaged pixel centre as a suggested `opticalCenter` (the UI
must save it). Test mode (`testImage`, `:484-526`) annotates one averaged frame (box, crosshairs, text) and pushes a
PNG.

### 1.6 Maths after the scan

- `surfaceFit` (`:649-694`, the `curveFitOpticalCalibration` action): least squares of
  `z = c0 + c1·x + c2·y + c3·xy + c4·x² + c5·y²` over **all 465 cells**, x and y in mm. Unmeasured cells count as
  zero error, so a partial-extents scan biases the fit. It computes R² and then drops it.
- `polyFit2D` (`:633-644`, order 4) exists but is commented out at the call site (`actions.py:292`).
- The firmware evaluates the curve without `c0`. It handles `c0` by shifting the motor positions instead
  (madgrizzle `Kinematics.cpp`, §1.8).

### 1.7 Outputs

| Output | How |
|---|---|
| `updateOpticalCalibrationError {errorX, errorY}` (31x15 float grids) | end of a horizontal scan; `reloadCalibration`; `calibrationError` setting request |
| `updateOpticalCalibrationCurve {curveX, curveY}` | `curveFitOpticalCalibration`; `calibrationCurve` setting request |
| `updateOpticalCalibrationFindCenter {opticalCenterX, opticalCenterY}` | find centre |
| `updateCalibrationImage {command, data}` (base64 PNG) | active image per square; test image |
| `cameraMessage` `cameraImageUpdated` / `updateCamera on|off` | live 256x192 stream from the capture thread (still in today's contract, `frontend/src/protocol/viewer.ts:23-26`, shown by `features/viewer/CameraOverlay.tsx`) |
| CSV | `saveCalibrationToCSV` (`:541-569`): `~/.WebControl/OpticalCalibrationResults/calibrationValues<YYYYmmdd-HHMMSS>.csv`, X grid then blank line then Y grid, rows top to bottom, 2 decimals, trailing commas |
| Settings | `saveAndSendOpticalCalibration` (`:571-605`) writes `calX*`, `calY*`, `xyErrorArray` with `config.setValue`. That syncs: `$O=x,y,ex,ey` x 465 plus `$O=31,15,42,42` (save to EEPROM) for $85, and `$87..$98=` for the curve, **only when the controller reports version ≥ 100** (`config.py:491-517`) |
| `adjustCenter ±5` | writes `motorOffsetY += dist` to Maslow Settings **directly**, which sends `$`, then `returnToCenter` (`actions.py:1338-1352`) |

### 1.8 Who consumed the result: nobody, today

- Only "custom" firmware (version ≥ 100, `Actions/firmware.py:10` folder `madgrizzle`) was meant to apply it. That is
  madgrizzle/Firmware `release/madgrizzle` (`bf4350f`, 2018-11-29, `VERSIONNUMBER 101.25`) and
  `optical-calibration-latest` (`95f7d4b`, same version). In that firmware:
  - `$O=x,y,ex,ey` fills `calibration.xError/yError[31][15]` (`System.cpp:569-572`).
  - `$44` enables the correction, `$46` picks matrix (default) or curve, and `$47..$58` are `calX0..calY5`
    (`Settings.cpp:441-486`, `Report.cpp:186-193`).
  - `Kinematics::_adjustTarget` adds the bilinearly interpolated grid error, minus the centre cell, or the curve
    without `c0`, to the target before the **triangular** inverse (`Kinematics.cpp:206-256`, `:270-273`). The centre
    error goes into the motor positions (`Kinematics.cpp:60-75`).
- **Key mismatch.** WebControl commit `84f99b0` (2019-08-13, "save and edit updates") renumbered the optical keys
  in `defaultwebcontrol.json`: `xyErrorArray` 45→85, `calX0` 47→87, and so on. That made room for holey's `$43-$46`.
  No madgrizzle branch read the new numbers (checked `release/madgrizzle`, `optical-calibration-latest`,
  `optical-calibration-2`, `master-merged`). On 101.25, `$87..$98` and `$144` hit `default:` → `Invalid statement`.
  So after August 2019 WebControl could still deliver the matrix (`$O`), but it could never enable the correction or
  send the curve.
- Holey firmware (H27 to H293, `spec/firmware.md:6-13`) has no `$O`, no `$44` optical meaning and no
  `_adjustTarget`. The owner's controller and every M6 upgrade target ignore the result.
- Rust today: `wc-firmware/src/settings.rs:11-23` keeps `$85/$86/$144` unsendable and sends `$87..98` only to firmware
  ≥ 100 (`:119`, `:276-279`). Nothing in `wc-kinematics` or `wc-gcode` applies an XY correction. Python didn't either:
  the correction was only ever meant to run in the firmware.

**Consequence:** the port can't promise "calibration corrects your cuts" without one of: a holey geometry fit (§2.7,
recommended), host-side correction, or new firmware. That choice is Q1.

### 1.9 Python bugs and hazards (not to be ported)

1. **OpenCV ≥ 4.5.1 angle convention.** `requirements.txt` at `220aa0e` pins `opencv-python==4.6.0.66`. There,
   `minAreaRect` reports angles in (0, 90], not OpenCV 3's [-90, 0). The `_angle` logic was written for OpenCV 3.
   So in calibration mode an axis-aligned square comes back as 90° and its offset is rotated a quarter turn. Probe run
   (this plan, 640x480, 120 px square at (350, 220), centre (320, 240), true offset (+3.81, +2.54) mm):

   | Square tilt | Python 4.6 raw (Dx, Dy) | With an OpenCV-3 angle shim |
   |---|---|---|
   | 0° | (2.54, −3.81) **wrong** | (3.81, 2.54) |
   | −3° | (2.87, −3.42) **wrong** | (3.76, 2.43) |
   | +3° | (3.58, 2.81) | (3.58, 2.81) |

   Find-centre mode skips the rotation and is unaffected. Any scan run on the pinned stack would have stored garbage
   for about half the squares.
2. `optical_onStart` calls `on_Start`, which does not exist (`actions.py:257`), so it always alerts.
3. Measure-only mode writes `self.measuredErrorsX`, which is never initialised (`:365`). Nothing turns the mode on.
4. A vertical scan never pushes the error grid at the end (`:452-457`).
5. `HomeIn` leaves the controller in **G91** (`:203`) and never restores G90.
6. Re-homing has no attempt limit and no step limit (`:358-360`). One misdetection can command a long G0.
7. A failed detection silently ends the scan with `inAutoMode` still set (`:380-381`). The next start then takes the
   "advance" branch.
8. The `HomingY -= 7.0` guess (`:418`, `:449`) and the fit's zero-filled unmeasured cells (§1.6).
9. Image processing runs on the serial message thread (`messageProcessor.py:83`) and blocks controller input for
   ≥ 1 s per square.
10. `adjustCenter` changes `motorOffsetY` without any confirmation. Start silently saves settings.
11. It switches the **UI units setting** (broadcast) to run in inches. `HomeIn` logs the literal `"message"` (`:195`).
12. It assumes the camera is mounted with image +x = machine +X and image up = machine +Y. A camera turned 180° makes
    the loop diverge. Nothing checks this.

---

## 2. Proposed Rust/React design

### 2.1 Principles

- Experimental and off by default. It is compiled behind the `optical` cargo feature (on in release builds, Q2) and
  enabled at runtime only by `--optical` (Q3). When it is off, the endpoints return 404, the actions alert "Optical
  calibration is not enabled", and the UI hides the screen. **No new keys in `webcontrol.json`**, so
  `wc-config`'s v0.94 byte-for-byte round trip (`rust/README.md`, `python_roundtrip`) is untouched.
- The machine core stays sans-IO. Capture and image work run in the shell / blocking pool, like `Work`
  (`wc-machine/src/work.rs`), so Stop never waits behind a frame.
- Every motion goes through the existing queue and gates. Nothing is written to settings or the controller until an
  explicit **Accept**.
- Port the measurement faithfully, with the §1.9 fixes. Golden-test it against Python where Python was right.

### 2.2 Crates and files

```
rust/crates/wc-optical/            new; depends on image, imageproc, nalgebra, wc-kinematics (fit), serde
  src/lib.rs                       re-exports; no IO
  src/frame.rs                     Gray frame (w, h, Vec<u8>), from YUYV / I420 / RGB / JPEG / PNG
  src/detect.rs                    the §1.5 pipeline: blur, canny, close, contours, simplify, min-area rect, offsets
  src/measure.rs                   10-frame average with the ±2σ cut; Measurement {dx_mm, dy_mm, px_centre, angle, frames_used}
  src/grid.rs                      Cell (i, j), extents, serpentine order, nominal mm, the [i+15][7-j] index
  src/fit.rs                       6-term surface fit (+ R²), measured cells only; optional order-4 fit
  src/codec.rs                     xyErrorArray encode/parse (int truncation as Python), CSV text
  src/overlay.rs                   annotated PNG (box, crosshairs; numbers go in JSON, not burned-in text)
  src/render.rs                    synthetic target renderer (tests and the synthetic camera)
  src/camera/mod.rs                trait Camera { fn start(&mut self, cfg) ; fn next_frame(&mut self, deadline) -> Result<Frame> ; fn stop }
  src/camera/file.rs               frames from a directory of PNGs (looping), for tests and demos
  src/camera/synthetic.rs          renders what a camera at the sim's true sled position sees
  src/camera/v4l2.rs               feature "v4l2": USB/UVC webcams via the `v4l` crate (mmap stream; YUYV preferred, else MJPEG)
  src/camera/rpicam.rs             feature "rpicam": spawns `rpicam-vid … --codec mjpeg -o -`, splits JPEGs on SOI/EOI
rust/crates/wc-machine/src/optical.rs   the scan state machine (§2.4) and the optical actions
rust/crates/wc-server/src/camera.rs     the capture task: owns the Camera, serves frames to the scan and the live stream
rust/crates/wc-server/src/optical.rs    the /api/calibration/optical* routes
rust/crates/wc-protocol/src/slices/calibration.rs   optical messages and bodies (additive)
```

Cargo features: `wc-optical/{v4l2,rpicam}`, and `wc-server/optical = ["wc-optical", "wc-optical/v4l2",
"wc-optical/rpicam"]`. `image` uses `default-features = false, features = ["png", "jpeg"]`. CI builds and tests
with `--features optical` on Linux and runs the aarch64 zigbuild with it. `spec/RUST-PLAN.md:191,206` records that
`v4l` 0.14 + `imageproc` 0.25 already cross-compile with no system libraries.

### 2.3 Cameras on a Pi 3B with Pi OS Lite 64-bit trixie

| Camera | Stack on trixie | Backend | Why |
|---|---|---|---|
| USB webcam (UVC) | V4L2 `/dev/videoN` | `v4l` crate, mmap streaming | Pure ioctl wrapper; cross-compiles with zigbuild (spike). Takes YUYV, whose luma bytes are the gray image with no decode; else MJPEG, decoded by `image`'s pure-Rust JPEG |
| Pi Camera Module (CSI) | libcamera only; the old bcm2835-v4l2 driver is gone, and `/dev/video0` from unicam is raw Bayer with no ISP | **subprocess `rpicam-vid`** (rpicam-apps, preinstalled on Pi OS) with `--codec mjpeg --width W --height H --framerate F -t 0 --nopreview -o -` | Linking libcamera (`libcamera` crate) needs its C++ headers and pkg-config at build time, which breaks the sysroot-free zigbuild. A subprocess needs no build dependencies, and the process is killed on stop. MJPEG avoids I420 stride padding. Fallback with no code: run a V4L2 app under `libcamerify` |
| Tests, CI, demos | none | `file`, `synthetic` | No hardware anywhere in CI |

Selection: `--camera auto|/dev/videoN|rpicam|synthetic|dir:<path>`. `auto` tries `rpicam-hello --list-cameras`,
then the first V4L2 capture device. The resolution comes from the existing Camera Settings `videoSize` / `fps`. Only
the capture task opens the device. The live overlay (`toggleCamera`, `cameraMessage`) and the scan share it: one
reader, frames fan out with a sequence number and a capture `Instant`. The scan takes only frames captured after the
settle deadline, which fixes Python's stale-frame race (`webcamVideoStream.py:146-154`). The camera stops when nobody
needs it.

### 2.4 Motion sequencing in `wc-machine` (`optical.rs`)

A state machine on `Machine`, driven by events; the shell's camera task does the IO:

```
Idle -> Raising (Raise to safe height, existing clearance::Raise) -> Moving(cell, attempt) -> Settling(deadline)
     -> Capturing (Effect::Capture{n:10, after}) -> Measuring (Work::OpticalMeasure on the blocking pool)
     -> (|d| >= tol && attempt < max) ? Moving(cell, attempt+1) : Recorded(cell) -> next cell | Done
```

- **Lines.** It moves in mm whatever the UI units are: `G21`, `G90`, `G0 X Y` (or G1 at a configured feed, Q8),
  then the `B10 L` barrier with a new `setup::Measure::Optical` variant (`wc-machine/src/setup.rs:32-40`,
  `:164-177`). At the end it restores the positioning mode and units it found: `G20` if the machine was in inches,
  and always `G90`. It never touches the UI units setting (fixes §1.9.5 and §1.9.11). The barrier works on every
  firmware and in `wc-sim` (`wc-sim/src/lib.rs:331`).
- **Gates.** Every optical action that moves or captures is `Gate::Idle` (`wc-machine/src/gates.rs:69`): refused
  while streaming, paused, probing (`actions.rs`'s `SAFE_WHILE_PROBING`), while the firmware upgrade holds the link,
  and when not connected (`MACHINE_COMMANDS`). Jobs, jogs, `moveTo` and the other optical actions are refused while
  a scan runs ("Optical calibration is running; press Stop first"). Stop is not refused.
- **Stop.** `stop_run` (`job.rs:286`) ends the scan: the state goes to Idle, any capture is cancelled, the partial grid
  is kept for review (it is never auto-saved), and units are restored after the resync.
- **Alarm.** `latch_alarm` (`alarm.rs:83`) aborts the scan. The scan's queued lines are ordinary `Queued::Line`s
  that do not survive (`alarm.rs:48`). Do **not** use `Queued::Mark` for the barrier, because marks survive alarms.
- **Bounds** (fixes §1.9.6 and §1.9.7). A per-cell attempt limit (default 6), a per-step limit (default 10 mm), and a
  total-correction limit (default 25 mm from nominal) each fail the cell. A failed or undetected cell **pauses the
  scan** with an Alert offering Retry, Skip or Stop. It never silently ends. Targets outside the work area
  (`bedWidth`/`bedHeight`) are refused before the scan starts.
- **Orientation pre-check** (fixes §1.9.12; new). Before the first cell it moves +5 mm X, then +5 mm Y, and measures
  the image displacement each time. That gives the camera's rotation, any mirroring and its px/mm. If they disagree
  with the marker-size scale by more than 5%, or the camera is mirrored or turned more than 15°, it refuses with an
  explanation. The measured rotation replaces the angle heuristic.
- **First guess** for the next cell is the previous cell's correction, as Python did, but without the −7 mm.
- **Single-cell actions:** Test image (capture and annotate, no motion), Find centre (no motion), and Measure here
  (one cell's closed loop at the current grid cell).
- **Not ported:** `optical_onStart` (dead), and `adjustCenter`, which edited `motorOffsetY`. The "Return to centre"
  button reuses `returnToCenter`.
- **Result** lives in the machine as `OpticalSession {grid: 31x15 Option<Measurement>, extents, started, settings
  used}`. It is pushed incrementally (`opticalScanProgress`) and saved only on Accept.

### 2.5 Accept and settings

`POST /api/calibration/optical/accept` with `{use: "matrix" | "geometry"}` (geometry only after Track F). It is
refused during a job, during a scan or with nothing measured (409, like `calibration.rs`'s `NOTHING_TO_ACCEPT`). The
UI first shows a confirm dialog with what will be written:

- `matrix`: writes `xyErrorArray` (Python's encoding) and `calX*`/`calY*` (the measured-cells-only fit) through the
  existing settings path. For holey firmware that writes nothing to the controller, and the dialog says so
  ("stored; your firmware does not apply it"). For firmware ≥ 100 the existing `wc-firmware` rules apply and `$O`
  is still never sent (Q4).
- `geometry`: the holey fit's four values, sent through the same `Accept` / `SettingWrite` path as holey calibration
  (`wc-calibration/src/accept.rs`), with the same safe-height raise.

Settings → "Save settings" stays a separate explicit PUT, as in Python's new UI (`api/calibration.py:222-232`). Start
no longer saves them silently: the scan uses the settings in the request and records them in the session.

CSV: `GET /api/calibration/optical/csv` downloads Python's format (§1.7) in the browser instead of writing to the Pi.

### 2.6 Protocol and contract (bump `contract/VERSION` 3 → 4)

Restore from `59895dd` with these changes (`frontend/src/protocol/calibration.ts`, `actionArgs.ts`, `http.ts`, then
`npm run contract`; `wc-protocol` events and slices):

| Item | Change from 59895dd |
|---|---|
| Event `opticalCalibration {data:{command, arg: settings}}` | Restored. Commands: `optical_Calibrate`, `testOpticalCalibration`, `findCenterOpticalCalibration`, plus new `measureOpticalCell` (arg also carries `{i, j}`) |
| Actions | Keep `stopOpticalCalibration` (alias of Stop for the scan), `reloadCalibration`, `clearCalibration` (clears the session only), `curveFitOpticalCalibration`, and new `retryOpticalCell`, `skipOpticalCell`. **Drop** `optical_onStart`, `adjustCenter` and `saveCalibrationToCSV` (now HTTP). `saveOpticalCalibrationConfiguration` stays `via: none`. `saveAndSendOpticalCalibration` → replaced by the accept endpoint |
| Messages | Keep `updateOpticalCalibrationError`, `updateOpticalCalibrationCurve`, `updateOpticalCalibrationFindCenter`, `updateCalibrationImage`. Add `opticalScanProgress {state, cell, attempt, done, total, measurement?}` and `opticalOrientation {rotationDeg, mirrored, pxPerMm}`. The messages are broadcasts (several UIs may watch a scan) |
| HTTP | `GET /api/calibration/optical` (settings, curve, error, session, camera availability), `PUT /api/calibration/optical/settings`, new `POST /api/calibration/optical/accept`, new `GET /api/calibration/optical/csv` |
| `GET /api/state` | optional `experimental: string[]` (contains `"optical"` when enabled). Optional, so Python-era schemas stay valid |
| `toggleCamera` / `queryCamera` / `statusRequest "cameraStatus"` | Implemented when optical is enabled; still no-ops otherwise (`wc-machine/src/actions.rs:110`) |

The contract tests then fail where they should: `wc-machine/tests/contract.rs`, `wc-protocol/tests/protocol.rs:174`
(which asserts the event is unknown), `wc-machine/tests/transcripts.rs:198` (asserts 404) and
`frontend/src/contract/usage.test.ts`. Each changes with its track.

### 2.7 What the result can feed (owner decision Q1)

| Option | Works on the owner's firmware? | Size | Risk |
|---|---|---|---|
| A. Measure and report only (heat map, CSV) | yes | small | none |
| **B. Holey geometry fit from the optical grid** (recommended next to A) | **yes**: Accept writes motor spacing, motorOffsetY and the chain tolerances, as holey calibration does | ~400 lines in `wc-calibration` | Low. It reuses `trf.rs` and `wc_kinematics::Geometry` with up to 465 absolute points instead of 12 distances. One caveat: the grid is relative to where the sheet is pinned, so the fit needs two extra unknowns for the sheet's offset and rotation |
| C. Host-side correction (pre-warp G-code XY before streaming) | yes | large: line segmentation, arcs, restart/index mapping, preview, MPos inverse-mapping | High. It touches the job path that M4 hardened |
| D. Port `_adjustTarget` into a 51.292 fork with non-conflicting keys | after a flash | firmware work plus M6 migration changes; 1.86 KB of the Mega's 8 KB RAM | High; the owner can't test it without a camera |

---

## 3. Testing without hardware

1. **Renderer** (`wc-optical/src/render.rs`). It draws the target (squares of `markerX` at 3" × `scale` pitch, black
   on white) as seen by a camera centred at the sled's **true** position. Parameters: px/mm, rotation θ, mirroring,
   optical-centre offset, radial distortion k1, Gaussian blur σ, Gaussian noise, a vignetting/light gradient, JPEG
   quality, and partial occlusion. Rasterising by exact area coverage (4x4 supersampling) avoids the half-pixel bias
   of `fillPoly` (Python's own tests are off by 0.5 px for that reason; `test_optical_calibration.py:62`). A
   deterministic xorshift RNG means no `rand` dependency.
2. **Ground-truth tests** (`wc-optical/tests/detect.rs`): about 60 rendered cases, each with a known offset. The
   detector should be within 0.3 px / 0.05 mm when sharp, 0.5 px blurred or noisy, and get the rotation to ±0.5°.
   Blank frames, frames with no square and frames where the square leaves the image must be rejected (Python's
   `test_optical_calibration.py:68-96` cases).
3. **Goldens against Python** (feasible; checked for this plan). The repo's `.venv` already has Python's exact pins
   (`cv2 4.6.0`, `numpy 1.23.5`, Python 3.9.6).
   `tools/optical/golden.py` reads `Actions/opticalCalibration.py` from `git show 220aa0e:...` into a temp dir,
   stubs `DataStructures/makesmithInitFuncs.py`, replaces `time.sleep`, and feeds the Rust-rendered PNGs through a
   FakeCamera (the same trick as `test_optical_calibration.py:25-44`). It records `processImage` for find-centre and
   calibration modes, each twice: on raw OpenCV 4.6, and with a shim that maps `minAreaRect`'s angle into OpenCV 3's
   range. It also records `surfaceFit`, `polyFit2D`, `saveAndSend`'s string, `parseErrorArray` and the CSV on
   seeded random grids. Output goes to `rust/crates/wc-optical/tests/fixtures/python_golden.json`; the regenerate
   pattern is `wc-calibration/tests/fixtures/regenerate_trf_path.py`. Rust must match find-centre within 0.5 px and the
   **shimmed** calibration mode within 0.5 px / 0.05 mm. The fit must match to 1e-9 and the codec/CSV byte for byte.
   The raw-4.6 rows are kept as documented divergences (§1.9.1). `imageproc` and OpenCV differ in Canny (L1 against
   L2 gradient) and contour tracing, which can move edges by about a pixel. The ±0.5 px budget is on the 4-corner
   centre, and averaging helps. If that budget can't be met, the fallback (Q7) is to golden only the ground-truth
   cases.
4. **Sim-driven closed loop** (`wc-machine/tests/optical.rs` and `wc-server/tests/optical.rs`). `wc-sim` already
   tracks position (`wc-sim/src/lib.rs:504`). The synthetic camera reads `sim.position()` through an injected **true
   error field**. The realistic field is a geometry mismatch: true sled = `forward(true geometry, inverse(configured
   geometry, commanded))` with `wc-kinematics`, e.g. motor spacing off by +3 mm and motorOffsetY off by −5 mm. Assert:
   the scan converges, the grid equals the field at the cells (±0.1 mm), the attempt and step limits trip on a field
   with a 30 mm jump, the orientation check refuses a mirrored or 180° camera, Stop mid-scan leaves no queued motion
   and restores G90/units, an alarm mid-scan aborts, every optical action is refused in each run state
   (`tests/gates.rs` style), and the geometry fit (Track F) recovers the injected +3 / −5 mm to 0.5 mm.
5. **Transcripts** (`contract/serial/transcripts/`): `optical_scan_two_cells_mm.json` (exact bytes: `G21`, `G90`,
   `G0 X… Y…`, `B10 L`, re-home, restore), `optical_stop_mid_scan.json` and `optical_alarm_mid_scan.json`, replayed
   by `cargo test -p wc-machine --test transcripts`.
6. **Camera backends.** `rpicam.rs` is tested against a fake `rpicam-vid` script on `PATH` that writes
   concatenated JPEGs to stdout, then truncated output, then exits. `v4l2.rs` stays a thin adapter over the pure
   `frame.rs` converters, which are unit-tested. The adapter is covered only on hardware: it is excluded from the 95%
   gate with a reason in `scripts/coverage.sh` (Q6), and gets a manual checklist
   `spec/rust-checklists/M7.md` for testers.
7. **UI.** Vitest from the restored `OpticalCalibration.test.tsx` and `opticalForm.test.ts`, plus progress, Accept and
   hidden-when-disabled tests. Playwright `frontend/e2e/optical.spec.ts` runs against
   `webcontrol-rs --simulate --optical --camera synthetic`.

---

## 4. Work breakdown

**Step 0 (one PR, sequential, ~600 source lines):** freeze the interfaces so the tracks don't collide.
- Contract and protocol per §2.6 (`frontend/src/protocol/calibration.ts`, `actionArgs.ts`, `http.ts`,
  `main.ts` `experimental`, `contract/` regenerated, `VERSION` 4, `contract/README.md` notes).
- `wc-protocol` types and events.
- The `wc-optical` crate skeleton: `Cargo.toml` with features, and `lib.rs` with the public types (`Frame`,
  `Measurement`, `Cell`, `ErrorGrid`, `OpticalSettings`, the `Camera` trait, `DetectParams`) and `todo!()` bodies.
- `--optical` / `--camera` flags in `wc-server/src/cli.rs`.
- Stub routes returning 404 when disabled.
- `spec/RUST-PLAN.md` owner-decision line updated.

Then five tracks in parallel. Each owns only the files listed, has a 95% per-crate coverage target, and must pass
clippy and fmt.

| Track | Owns | Acceptance |
|---|---|---|
| **A. Vision core** | `wc-optical/src/{frame,detect,measure,grid,fit,codec,overlay}.rs`, `wc-optical/tests/{detect,fit,codec}.rs` | Ground-truth cases (§3.2) pass; Track B's goldens pass (find-centre ±0.5 px, shimmed calibration ±0.5 px/0.05 mm, fit 1e-9, codec/CSV byte-exact); one 640x480 frame processed in < 150 ms on aarch64 in Docker (Pi 3B budget) |
| **B. Synthetic target and goldens** | `wc-optical/src/render.rs`, `wc-optical/src/camera/{file,synthetic}.rs`, `tools/optical/golden.py`, `wc-optical/tests/fixtures/**`, `wc-optical/tests/render.rs` | Renderer deterministic (hash-pinned PNGs); `golden.py` regenerates `python_golden.json` from a clean checkout with a documented command; fixture README explains the raw/shim split |
| **C. Cameras and live stream** | `wc-optical/src/camera/{mod,v4l2,rpicam}.rs`, `wc-server/src/camera.rs`, the camera flags in `wc-server/src/cli.rs` (already stubbed), `spec/rust-checklists/M7.md` | Fake-`rpicam-vid` tests; frame fan-out with sequence and timestamp; `toggleCamera`/`queryCamera`/`cameraStatus` drive `cameraMessage` at about 2 fps at 256x192; the camera closes when idle; aarch64 zigbuild with `--features optical` passes in CI; `ldd` shows no new shared libraries |
| **D. Scan sequencing and API** | `wc-machine/src/optical.rs`, `wc-machine/tests/optical.rs`, additive lines in `machine.rs` (Event/Effect variants), `actions.rs` (dispatch), `gates.rs`, `alarm.rs`, `setup.rs` (`Measure::Optical`), `work.rs` (an optical lane); `wc-server/src/optical.rs` and its route registration; `contract/serial/transcripts/optical_*.json` | §3.4 sim tests and §3.5 transcripts pass; `--test gates` and `--test latency` still pass (Stop reaches the port within 5 ms during a measurement); Accept writes nothing before confirm; CSV matches Python's format |
| **E. UI** | `frontend/src/features/calibration/{OpticalCalibration,OpticalFields,Charts,chartData,opticalForm,fixtures,api}.*` (restored from `59895dd^` and adapted), `calibration.css` optical rules, the `CalibrationPage.tsx` / `setupLinks.ts` entries, `frontend/e2e/optical.spec.ts` | Screen hidden unless `experimental` includes `optical`; "Experimental" banner saying what the result does on the connected firmware; live progress grid / heat map; Retry/Skip/Stop on a failed cell; Accept dialog listing every setting it writes; CSV download; no `adjustCenter`; vitest 95%; e2e against `--simulate --optical --camera synthetic` |

Dependencies: A and B are needed for D's end-to-end tests. D can develop against `file` frames and a stubbed
detector first, and E against `fakeFetch` and the fake socket. C is independent.

**Track F (after Q1 = B; owner decision):** `wc-calibration/src/optical_fit.rs`. It fits motor spacing,
motorOffsetY, left/right chain tolerance, plus the sheet's x/y offset and rotation, to the measured grid through
`trf.rs`. It plugs into `Accept` with `use: "geometry"`. Acceptance: it recovers the injected geometry in §3.4; a
Python cross-check isn't possible (Python had no such fit), so it is pinned by its own iteration-path fixture like
`trf_path.json`.

Rough size: Step 0 ~600, A ~900, B ~500 (+ Python ~150), C ~600, D ~900, E ~700 (frontend), F ~400. Each fits one
PR under the 1000-line source cap. D may split into "scan" and "accept/routes".

---

## 5. Risks and open questions

**Risks**

- **No hardware in the loop.** Detection, settling and lighting are only modelled. The first tester's run is the real
  validation. Mitigation: the M7 checklist, the CSV export, and a diagnostic capture bundle (the 10 raw frames per
  cell, optional) to send back.
- **Python parity is partly undesirable** (§1.9.1). Goldens use the OpenCV-3 shim, so "matches Python" means
  "matches what Python meant".
- **imageproc vs OpenCV** differences (Canny norm, contour tracing, approxPolyDP tie-breaks) may exceed ±0.5 px on
  soft images.
- **rpicam-vid** flags and output on trixie, and how fast it starts, are unverified on a Pi 3B. The width must be a
  multiple of 64 if raw I420 is ever used; MJPEG avoids that.
- **CPU and RAM on a Pi 3B** (1 GB): 10 frames x 640x480 per cell is light. 1024x768 at 5 fps plus the live stream
  needs measuring (the Track A budget).
- **A result that does nothing.** Without Track F (or C/D), a tester may believe the cuts are corrected. The UI must
  say otherwise.
- **The target.** A 90" x 42" grid is hard to print accurately, and `scale` only absorbs uniform scaling.
- **Safety.** A misdetection now trips a bounded step or attempt limit instead of driving the sled away, and the
  orientation check catches the wrong camera mounting. Any scan still moves the sled across the whole sheet unattended.

**Open questions for the owner**

1. **What should the result feed?** A only; A + B (recommended); C; or D (§2.7)?
2. Compile `optical` into the release binary (+1-2 MB) with a runtime flag, or ship a separate build?
3. Runtime gate: the `--optical` CLI flag in the systemd unit (recommended; no `webcontrol.json` change), or a UI
   setting (a new key in the shared `webcontrol.json`, which Python v0.94 would carry around)?
4. For firmware ≥ 100, keep refusing `$O` and `$87..98`, since no firmware reads those numbers? Or remap to
   101.25's `$44/$46/$47..58`, which collide with holey's `$43-$46` meaning? Recommendation: refuse, and say so.
5. Keep Python's action names for contract continuity (the plan does, minus the dead ones), or rename them cleanly
   while the contract is at a version bump anyway?
6. Accept excluding the hardware camera adapters from the 95% coverage gate?
7. If imageproc can't hold ±0.5 px against OpenCV on blurred goldens, is "ground truth ±0.3 px" acceptable as the
   bar instead?
8. Move with `G0` (Python) or `G1` at a slower feed to reduce sled sway? Settle delay default 1 s?
9. Target: should the plan include a generator for tiled printable sheets (SVG/PDF with registration marks), or will
   testers bring their own grid? And is there a camera-to-bit offset to record, or is the camera assumed at the sled
   reference point, as Python assumed?
