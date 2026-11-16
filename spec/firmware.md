# Maslow firmware vs WebControl backend

Findings from reading the firmware source, 2026-11-14. This is the reference the Rust backend and the firmware
upgrade flow are built against. Firmware paths are relative to `cnc_ctrl_v1/`.

| Tag | Repo @ commit | Notes |
|---|---|---|
| S126 | MaslowCNC/Firmware @ e1e0d02 | Stock 1.26 |
| H27 | madgrizzle/Firmware @ 950fb23 | Holey 51.27, bundled by the Python backend's `buildFirmware.sh`; the user's controller |
| H28 | madgrizzle/Firmware @ e9cc445 | 51.28: FAKE_SERVO handling and version only |
| H291 | Orob-Maslow/Firmware @ 97d12a6 | Tags v51.29 / v51.291 (master); reports `51.29` |
| H292 | Orob-Maslow/Firmware @ 0064955 | Tag v51.292; the planned upgrade target |
| H293 | Orob-Maslow/Firmware @ 300f6c7 | Head of 291_testing; reports 51.293, differs in Z-limit defaults, encoder pins, B04 |

## 1. Serial protocol

- **Link:** 57600 baud 8N1 in every version. Lines in end at `\n` (`\r` is stripped); lines out end `\r\n`.
- **Buffer:** `INCBUFFERLENGTH 128`, a ring buffer holding 127 bytes. WebControl's `bufferSize = 126` is safe; it
  counts the appended space and `\n` exactly as stored. `?` and the realtime characters never enter the ring.
- **Case:** every version trims each line and upper-cases it before parsing it (`maslowRingBuffer::prettyReadLine`,
  `maslowRingBuffer.cpp:102-112`, from `gcodeExecuteLoop`: S126 `GCode.cpp:622`, H27 `:685`, H292 `:774`), so `g20`,
  `m0`, `m6`, `t2`, `g91`, `g38.2`, `b05` and `$rst` act exactly as their upper-case forms.
- **Over-long lines:** a line over 126 characters overflows the ring. The firmware prints `Buffer overflow!`, stops,
  flushes, and then **executes the unread tail of the line as a new command** and acks it.
- **Echo:** the main loop takes a whole line out of the ring (`gcodeExecuteLoop`, H27 `GCode.cpp:685`), then
  prints it before running it: a B line whole, a G/M line one word group at a time, a line with no G or M whole
  (`interpretCommandString`, H27 `GCode.cpp:629-667`; the same in S126 and H292). `$` lines are the only ones not
  echoed. No status report can come between taking the line and its echo.
- **`ok`:** one per line, sent when the line *finishes* (moves, dwells and pauses block first). No `ok` for a line
  that was stopped (`!`, ALARM, overflow, failed probe, `$RST`). Boot sends one unsolicited `ok`.
- **`error:`:** `error: <text>` — `Bad number format`, `Invalid statement` (unknown B code or `$` key),
  `Please set $12, $13, $19, and $20 to load old position data.`, `EEPROM read fail. Using default settings.` (every
  upgrade or downgrade), `Invalid gcode ID:<n>`, and `error: probe did not connect` (which stands in for the `ok`).
  Unsupported G/M codes are not errors: `Command Gnn unsupported and ignored.` then `ok`.
- **Realtime characters:** `!` stops, flushes, turns the spindle off and gets an immediate `ok` (the running and
  flushed lines get none). `~` resumes and gets an immediate `ok`. Backtick and 0x18 soft-reset (51.27+). They act as
  realtime commands anywhere in a line, and a `\n` right after `!` or `~` is dropped (`quickCommandFlag`).

## 2. Status reports

```
<Idle|Pause|Stop,MPos:x,y,z,WPos:0.000,0.000,0.000>
[PE:lErr,rErr,space,lChain,rChain]    holey
[PE:lErr,rErr,space]                  stock
```

No Run state (it says Idle while moving). MPos is in current units with 2 decimals (0.254 mm resolution under G20).
No version sends `PWR:` or `F:`, so that parsing in `UIProcessor.py` is dead code. Reports are at least 200 ms apart.
`space` is the firmware's true free space (`incSerialBuffer.spaceAvailable()`, 127 with the ring empty, including
while a line runs, since it has left the ring). Every report comes from `execSystemRealtime()`, which reads the
serial port into the ring first (H27 `System.cpp:567-568`), and moves, dwells (`maslowDelay`), M0 pauses and the
forward solve all call it, so a byte that has reached the board is in the ring by the next report. B04 prints a
status line with no newline.

## 3. Version reporting

Never printed at boot. Only `B05` replies `Firmware Version <n>`, with 2 decimals, so every 51.29x build reports
`51.29` (fingerprint by `$$` contents and message quirks). WebControl queues B05 on connect and parses the reply, but
sends it only to the console, which is why the classic Controller Messages pane never shows it. A UI should forward
the raw line, re-send B05 on any boot banner (`PCB v1.`, `Grbl v1.00`, `ready`), and parse
`^Firmware Version\s+([\d.]+)`.

## 4. Commands WebControl sends

- B02, B04, B05, B06, B08, B09, B10, B11, B13, B14 work on every version; B99 on 51.27+.
- **Z limits B17–B20:** `Invalid statement` on 1.26/51.27/51.28. On 51.291 B17/B18 overwrite `$43`/`$44`. On
  51.292 B17/B18 store nothing but report success; only `$63`/`$64` (mm) work, and B20 reads them back.
- G17 is unsupported (sent by both calibration patterns). G38.2 moves at a fixed 60 mm/min, but touchZ's `F1` still
  sets the modal feed to 1 mm/min (without F it clamps the modal feed to zMaxFeed). G38.2 leaves the modal
  motion mode alone. On a touch G38.2 zeroes Z itself (`zAxis.set(0)`, H27 `GCode.cpp:922`, H292
  `GCode.cpp:1047`), prints `z axis zeroed` and acks, so a `G10 Z0` after it is redundant and WebControl sends none;
  on a miss it prints `error: probe did not connect`, `program stopped`, `z axis not set` and an empty line, with no
  `ok`, and sets `sys.stop`, which drops every line the firmware held (H27 `GCode.cpp:936-937`, H292
  `GCode.cpp:1061-1062`). It acks with `ok` and no `z axis zeroed` when Z is not attached (`G38.2 gcode only valid
  with z-axis attached`), when the target equals the current Z (no move at all), and for `G38.<n>` other than `.2`
  (H27 `GCode.cpp:836-948`). `M2` and
  `M30` only turn the spindle off. The firmware reads T only when it leads a line that also has a G or M word (`T2 M6`, `T2 G0 X1`):
  in `M6 T2`, `N10 T2 M6`, `G0 X1 T2` and a lone `T2` line the T is ignored, and a lone `T2` runs as a modal move.
- `$` keys: units match WebControl's assumptions. `$13` is the only path that sets R. `$37` is used only when
  `$46 ≤ 0`. `$40`/`$41` are percent. `$10`/`$11`/`$15` are unsigned ints. `$43` is unused on holey. `$86` and `$144`
  are rejected. `$18` (max Z RPM), `$9`, `$28` and `$36` were never sent before fix/firmware-sync; WebControl now adopts them from `$$` and syncs them. Firmware bug: `zKpPos` is never initialised
  (0 until `$29` is sent).
- Resync noise: `$45` and fractional `$10`/`$11`/`$15` are re-sent on every connect.

### How a `$n=value` is read, and the values the firmware refuses

Every version (S126, H27, H28, H291, H292, H293) reads both `n` and `value` with the same `readFloat`
(`NutsAndBolts.cpp`): it skips leading spaces, accumulates every digit into a 32-bit `long` (wrapping past ten
digits), multiplies a float32 `fraction` by 0.1 once per digit after the point, and stores `value * fraction` in
float32 (`value` alone when there is no point). `settingsStoreGlobalSetting` range-checks nothing. So trailing zeros
change the result: `1.000000` reads as 1.00000012, `1300.00` as 1300.00012, `63.5000` as 63.5000038, `0.28` as
0.28000003, while `1`, `1300`, `63.5` and `0.280` read as the nearest float32.

| Keys | Rule | Where | Effect of a value outside it |
|---|---|---|---|
| `$24`, `$28`, `$32`, `$36` (proportional weights) | 0 ≤ pOn ≤ 1 | `PID::SetTunings` (`PID_v1.cpp:101`), same in every version | the whole tuning is ignored; after `setupAxes()` (boot, `$38`) the PID keeps the zero gains `Axis::setup()` gave it and the axis never moves (`ALARM: The sled is not keeping up`) |
| `$21`-`$23`, `$25`-`$27`, `$29`-`$31`, `$33`-`$35` (gains) | K ≥ 0 | the same check | the same |
| `$7`, `$16`, `$17`, `$38`, `$39` (byte, bool, enum) and `$9`, `$10`, `$11`, `$15` (unsigned int) | whole numbers | the float is truncated on store | a fraction is dropped; `$39` is also clamped to 1..3 by `setPWMPrescalers` |

A live write of a refused tuning leaves the gains the PID had, so the damage shows only after the next boot. On the
owner's 51.27 (2026-11-15) Rust's full push had written `$24=1.000000` and `$32=1.000000`; `$$` showed
`1.00000011`, and no motor moved until `$24=1` and `$32=1` were written by hand.

WebControl therefore writes each value as the shortest text `readFloat` turns into the float32 nearest the intended
value: integers without a point, else the fewest fraction digits (up to nine significant) that read exactly, the
digits closest to the value when several do (`2438.4` goes out as `2438.3998`, `0.28` as `0.280`). A PID value is
first brought into its range. When no text reads exactly (beyond a 32-bit `long`), the closest one in range is sent.

### `$` writes with side effects

`settingsStoreGlobalSetting` (`Settings.cpp`) stores the value and ends every successful write with
`settingsSaveToEEprom()` (an EEPROM write; 51.292 prints `Change in setting, rewriting EEPROM`). It never compares the
new value with the old one, so writing a value the controller already holds repeats every side effect below.
Checked in S126, H27, H28, H291, H292 and H293; the switch is the same in all of them except where a column says so.

| Key | Side effect beyond storing | S126 | H27/H28 | H291 | H292/H293 |
|---|---|---|---|---|---|
| `$0`-`$8` | `kinematics.init()` | yes | yes | yes | yes |
| `$12` | `changeEncoderResolution()` on L and R (repoints `_encoderSteps`, sets the gearbox RPM resolution); `settingsLoadOldSteps()` once the old-settings flags clear; `kinematics.init()` | yes | yes | yes | yes |
| `$13` | sets `kinematics.R` (the only path, §6); `settingsLoadOldSteps()` as `$12`; `kinematics.init()` | yes | yes | yes | yes |
| `$19` / `$20` | `zAxis.changePitch()` / `changeEncoderResolution()`; `settingsLoadOldSteps()` as `$12` | yes | yes | yes | yes |
| `$21`-`$28` | `setPIDValues()` on L and R | yes | yes | yes | yes |
| `$29`-`$36` | `setPIDValues()` on Z | yes | yes | yes | yes |
| **`$38`** | **`settingsSaveStepstoEEprom()`, then `setupAxes()`, which re-runs `encoder.setup()` (reattaching the encoder interrupts), `motor.setupMotor()` (detached, PWM 0) and the PID setup for L, R and Z on the live board, reassigns the AUX pins and drives the laser pin low; then `settingsLoadStepsFromEEprom()`, each axis `write(read())` and `kinematics.init()`** | yes | yes | yes | yes |
| `$39` | `setPWMPrescalers()`: clears and sets the prescalers of timers 2, 3 and 4 (motor PWM) live; the same call boot makes, so the same value leaves them as they were | yes | yes | yes | yes |
| `$40`, `$41` | `kinematics.init()` | store only | yes | yes | yes |
| `$43`-`$46` | `kinematics.init()` | rejected | yes | yes | yes |
| `$60`, `$61` | prints `new spindle min=`/`max=` | rejected | rejected | yes | yes |
| `$63`, `$64` | prints `new zaxis upper limit=`/`lower limit=` | rejected | rejected | rejected | yes |

`settingsLoadOldSteps()` acts only in `STATE_OLD_SETTINGS` (a 1.26-era EEPROM): it sets each axis position from the
old step counts and wipes them. Every other key only stores.

**`$38` is the one key whose write re-initialises the axes.** On the owner's 51.27 PCB v1.1 (2026-11-15), after
Rust's connect wrote every setting, `$38` included, no motor moved: every XY move raised `ALARM: The sled is not
keeping up` within 0.5 s, and status reports showed MPos (the setpoint) advancing while the `[PE:]` chain lengths and
Z MPos never changed. Python v0.94 wrote only `$13` on the same machine minutes earlier, and it moved. So WebControl
never writes `$38` when `$$` shows the controller holds it, and after any `$38` write it resets the controller (B2).

## 5. Feed rate on Z moves

F is per minute in current units, stored as mm/min (×25.4 under G20), clamped to [1, `$15`], and modal (G0 sets it
too). G0 runs at `$15` and ignores F for speed. Z is clamped to `zMaxFeed = $18 × |$19|`: 39.9 mm/min on the 51.27
defaults, 399 mm/min on the 51.292 defaults. WebControl's restart sequence rapids without F (Rust: first to the
higher of the safe height and the bit's height, never lower, before any XY move), plunges with `G1 Z… F<plunge>`
(the file's last non-G0 Z feed, else 12.8 mm/min) and ends with `G0`/`G1 F<file F>` to restore the file's modal
motion and feed. If the file set no F before that point it restores no F (only `G0`/`G1` where the
file's motion differs from the restart's last move). With no plunge that leaves the firmware's modal feed alone; after
a plunge the plunge's 12.8 mm/min stays the modal feed, so the file's next F-less G1 runs at it. The boot feed rate is `$15/2` on 51.27/51.28, and 0
(clamped to 1 mm/min) on 1.26 and 51.29x.

## 6. Kinematics

- **R:** the constructor sets `R = 10.1` (Kinematics.h:39). Only writing `$13` sets
  `R = distPerRot / (2 × 3.14159)` (Settings.cpp:287 on H27, :296 on H292). `settingsLoadFromEEprom()` does **not**
  recompute R, so after every boot the firmware runs R = 10.1 until `$13` is written. WebControl used to send `$13` only
  when it differed, so the firmware ran 10.1; since fix/firmware-sync it writes `$13` once per controller boot.
- π is 3.14159 for R and the under-sprocket chain wrap. The firmware computes in float32.
- Chain density 0.14 × 9.8 / 1000 = 0.001372 N/mm. Holey calibration uses the same density and π, R =
  distPerRot / (2 × 3.14159) and `$45` from the settings; it ignores `$37` and `$7`, and always fits triangular. The
  firmware switches to the old triangular model when `$46 ≤ 0`.
- The clamp works on each axis independently. 51.29x also clamps inside the quadrilateral solve.
- Measured, before holey calibration took the firmware's values: the R and density differences each moved chain
  lengths by about 0.03–0.12 mm.

## 7. Pause, tool change, alarms

- M0/M1: the firmware prints `Maslow Paused` and waits for `~`. WebControl stops streaming after the line and sends
  `~` only on Resume, or before a jog when no job lines are buffered behind the pause.
- M6 with a new tool: spindle off, `Tool Change: Please insert tool N`, pause. `T2` and `M6` on separate lines keep
  streaming without a tool change and `M6 T2` stalls silently, so WebControl leads every M6 with the T the file
  selected, stops streaming after a tool-changing M6, and on Resume returns to the file's XY at safe height (Rust: or
  at the bit's height when that is higher).
- Manual Z (`$16=0`): `Message: Please adjust Z-Axis…` then pause; handled correctly.
- Alarms: "Sled not keeping up" sets `sys.stop` (H27 and H292 `Report.cpp`): the running line gets no ack, the ring is
  emptied, `loop()` reloads the position (`[Forward Calculating Position]`, `position loaded at:`) and then **takes
  new lines again**, so anything the host sends after the alarm runs. "Board invalid" hangs it. No other `ALARM:` is
  raised (the TLE9201 ones are compiled out; `Position Lost` and the gcode-parameter alarm are never reported).
  `Message: Unable to find valid machine position` comes from forward kinematics (boot, after every stop, after a
  geometry `$` write) and halts nothing, but means the position is not known.

## 8. Firmware upgrade

- Flash: `avrdude -patmega2560 -cwiring -P<port> -b115200 -D -Uflash:w:<hex>:i`. Flashing leaves EEPROM alone.
- The settings version is 5 (1.26), 6 (51.27/51.28), 7 (51.29x). Upgrading or downgrading **resets settings to
  defaults** (`error: EEPROM read fail`); step counts survive. 51.292 defaults: maxFeed 900, **`$18` = 126 (about
  10× the 51.27 Z speed)**, Z limits ±100 mm.
- 51.291's tagged source doesn't compile; use its shipped hex. Prefer the v51.292 tag over the 291_testing head.
- **Board-detection hazards in 51.29x:** the strap pattern that meant TLE9201 v1.6 in 51.27 means TB6643 v1.5b in
  51.29x, so an unmodified v1.6 TLE9201 board is driven with the wrong pins. One TLE5206 strap hangs. 51.292 swaps
  the TLE9201 left-encoder pins.

## 9. What 51.29x adds

A standalone `S` line sets spindle speed (only with the spindle on; `M3 S…` ignores S, and M3 leaves PWM at 0 until
an S line). R-format arcs (below). `$60`/`$61` spindle min/max; `$63`/`$64` Z limits (51.292). TB6643 boards. Text quirks:
` prescaler to ` appended to the boot line, `the value received:` after each `$` write, `ok` glued to a line after
`$60`/`$61` on 51.291, `Change in setting, rewriting EEPROM` on 51.292. Comments no longer hang the firmware (51.27
loops forever on one, so WebControl's comment stripping is load-bearing).

**R-format arcs.** 51.27, 51.28 and stock 1.26 never read R: `G2()` takes the centre as the start plus I and J, each
0 when absent, so an R arc has radius 0; `arc()` then takes no steps and `endMove()` sends the sled straight to the end
point at whatever speed the PID reaches (GCode.cpp `G2`, Motion.cpp `arc` on H27). 51.29x reads R only when it is
non-zero and X or Y differs from the start: centre = midpoint + e·√(R² − (d/2)²)·(−dy, dx)/d, with e = −1 for G2 and
1 for G3. `sq(R)` drops R's sign, so a negative R cuts the short arc, not the one over 180°; an R shorter than half
the chord gives a NaN centre; with no X/Y change it falls back to I and J. Every version's G2/G3 reads X, Y and Z as
absolute even under G91 (only G0/G1 honour it), ignores G17/G18/G19 (`Command G17 unsupported and ignored.`) and
cuts in XY. Like every word, R is found by the first `R` in the segment (the line is upper-cased, then split at each
G and M), and its number is the digits and punctuation after it, so `R 5` reads as R0. The G number is truncated
to an int, so G91.1 and G90.1 switch G0/G1 to relative and absolute like G91 and G90.

## Implications

### A. Current Python backend

| # | Severity | Finding |
|---|---|---|
| A1 | High | Lines over 126 characters are sent "alone" when idle; the firmware overflows and executes the line's tail. |
| A2 | Fixed | `$18`, `$9`, `$28`, `$36` were never synced; now adopted from `$$` and synced, and an EEPROM reset triggers a full restore. Install WebControl before flashing 51.292, or the 51.292 default `$18=126` could be adopted. |
| A3 | Fixed | Tool change: `~` was auto-sent; `T`/`M6` on separate lines ran on without a tool change; `M6 T2` stalled; a race could make Resume a no-op. |
| A4 | Fixed | Restart/resume plunged with G0 (at zMaxFeed) and left the modal F at the XY feed. |
| A5 | Medium | Z-limit commands do nothing on 51.27/51.28, overwrite `$43`/`$44` on 51.291, store nothing on 51.292. |
| A6 | Medium | When the firmware stops itself (ALARM, overflow), acks are lost and streaming blocks until Stop. v0.94's lost acks also held its manual queue, so a Home's tail waited for Stop; this fork's resync (35363b3) released it 0.25 s later (B17). |
| A7 | Fixed | touchZ's `F1` left the modal feed at 1 mm/min; G38.2 ignores F anyway. Without F, G38.2 still clamps the modal feed to zMaxFeed until Resume restores it. |
| A8 | Low | Holey calibration fits with R = distPerRot / (2 × 3.14159), but the firmware runs R = 10.1 until `$13` is written (§6); it ignores `$37` and `$7`. |
| A9 | Low–Med | The version only reaches the console; all 51.29x builds report 51.29. |
| A10 | Low | An unclosed `(` reaches the firmware and hangs 51.27; `!`, `~` and backtick inside a line act as realtime commands. |
| A11 | Low | `$45` and fractional `$10`/`$11`/`$15` are rewritten on every connect; NaN parses as 1. |
| A12 | Fixed | Nothing was sent if the board didn't reset on port open; now, with no banner or `ok` 3 s after the port opens, the handshake goes out anyway. |
| A13 | Fixed | M0/M1 timing depended on the early `~`; M0 at end of line wasn't detected. |
| A14 | Low | `$86`, `$144` rejected; `$43` unused on holey; G17 ignored. |
| A15 | Info | `bufferSize = 126` is right; PWR:/F: parsing is dead code; PE lines are mis-parsed until B05 is parsed. |
| A16 | Fixed | After a touching probe WebControl queued `G10 Z0` behind anything already queued; behind a Resume's restart it zeroed Z at the restart's height. The firmware zeroes Z on a touch, so none is sent. Resume and other moving actions pressed mid-probe are now refused until the probe answers or Stop. |
| A17 | Fixed | Every check WebControl made of a line (units, positioning and the home shift, M0/M1, T and M6, G38.2, `$RST`, the restart state, the preview's G words) was case-sensitive, so a lower-case `g20`, `m0` or `g91` acted on the controller but not in WebControl (upstream #177, #102). Lines are now upper-cased before they are read or written. |
| A18 | Fixed | R-format arcs (upstream #120) were streamed untouched, so 51.27 sent the sled straight to the end at PID speed (§9); now converted (B19). |
| A19 | Fixed | G2/G3 under G91 were streamed untouched; every firmware runs their X/Y (and Z) as absolute, while the position, restart and Z tracking took them as relative. Now written absolute, with Z refused (B19). |

### B. Rust backend requirements

1. 57600 8N1; send `\n`, parse `\r\n`; don't gate the first send only on the boot `ok`.
2. Treat any line containing `PCB v1.`, `Grbl v1.00` or `ready` as a boot. On each boot reset units, G90, tool and
   accounting, then B05 → units → G90 → `$$` → write only the keys `$$` shows differ (at the printed precision, B9)
   or hold a value the firmware refuses (a PID weight outside [0, 1] or a negative gain, §4, even when it compares
   equal), plus `$13` once (§6) → `$$` again only if a key other than `$13` was written. Every `$` value goes out as
   text `readFloat` reads as the nearest float32 (§4). `connect_repairs_a_pid_weight_above_one`. Keys `$$` does not report are not
   written; a blank one it reports is adopted. Never write a key the controller already holds: writing `$38` re-runs
   `setupAxes()` on the live board and the motors stop following ("`$` writes with side effects", §4). When `$38`
   must be written (it differs, an EEPROM reset, the C6 migration, a settings change, a restore), write it with the
   other `$` lines, then, once they are answered, close and reopen the port, which drops DTR and resets the Mega.
   Refuse every action and typed G-code that could move the machine until the next boot banner; if the port reopens without one (a
   board that does not reset on DTR), alert that the controller must be power-cycled and keep refusing until a banner
   comes. After the reboot `$$` shows `$38` held, so nothing restarts again; a `$38` that still differs then is not
   written again (one restart per attempt, with an alert). Never write `$38` while a job holds the machine; a `$38`
   line in a job's file is skipped with an alert.
3. Character-count flow control against 126 bytes; free on `ok`/`error:`; hard-reject lines over 126 characters;
   optionally resync from the PE `space` field when idle. Pace every line as v0.94's `_write` did (2026-11-15):
   a line due under 50 ms (`MINTimePerLine`) after the previous write waits a full 50 ms from when it became due,
   not the rest of the 50 ms; `!` and `~` are never paced. On the owner's cuts (51.27, router on) waiting
   only for the rest lost about 1 line in 290 (B18 sent each again), every one written 0–40 ms after the previous
   `ok`; v0.94's pacing lost none in 165,876 lines, and an air cut lost none under either.
   Adjustable for the loss-rate experiment (2026-11-16): `linePacingMode` (WebControl Settings) is `v0.94` (the
   above, default) or `minimum gap` (wait only until `linePacingGapMs` has passed since the last write), and
   `linePacingGapMs` is 0–500 (default 50; 0 never waits). Both apply from the next line, also mid-job through the
   run panel's `PUT /api/run/pacing`, which the job lock does not refuse; `!` and `~` are never paced (Rust never
   sends `?`: the firmware reports on its own). A segment is a job's stretch at one pacing: it starts with the job or
   a change and ends with a change, the job's end or Stop. Its lines (job lines written, not resends), losses (B18
   detections, so a line lost twice counts twice), lines lost twice, streaming time (pauses excluded, recoveries
   included), recovery time (from a lost line's write to its resend), lines/min, s/line and the median gap before
   each write whose line was acked within the gap plus 100 ms (pacing overhead apart from long moves) are pushed
   as `pacingStats` (every 2 s during a job) and kept for the last 10 segments. Each end writes one line, in this
   stable form, to the serial log, the service log and the console:
   `PACING SEGMENT mode=v0.94 gap_ms=50 lines=2284 lost=9 lost_twice=0 lost_pct=0.39 lines_per_min=507.2
   s_per_line=0.118 recovery_s=7.1 median_short_gap_ms=62 duration_s=270.3 file_line_start=1 file_line_end=2284
   ended_by=job_end` (`mode` is `v0.94` or `minimum_gap`; `ended_by` is `pacing_change`, `job_end` or `stop`;
   unknown numbers are `none`), then for a change `PACING CHANGE at file line N: v0.94 50 ms -> minimum gap 25 ms`.
   A job writes `PACING JOB START at file line N: <pacing>` and, at its end, a `PACING JOB` line with the same keys
   (its first pacing) and `PACING JOB END after file line N (<why>): <pacing>`.
4. Realtime: `!` resets accounting (immediate `ok`, no ack for the aborted line); `~` immediate `ok`.
5. Sanitise outgoing lines: strip comments, reject an unclosed `(`, remove `!`, `~`, backtick, `?` and control
   characters, and upper-case the rest (§1); read every line WebControl inspects (file, typed, restart, preview)
   upper-cased too. A restart needs an absolute X and Y: refuse one where the file moves X or Y relatively first.
6. ALARM or `Buffer overflow!` halts streaming and rewinds the job to the oldest unacknowledged line; acks are
   ignored until a status report (the resync); a probe `error:` is that line's ack;
   `$RST` has no `ok` of its own; detect a trailing glued `ok` on 51.291.
7. Parse every message type above, including B04's glued status line.
8. B05 after every boot; parse `^Firmware Version\s+([\d.]+)`; fingerprint 51.29x; record the PCB/driver banner.
9. Settings: per-version key/type table; compare at the printed precision; after every boot write what `$$` shows
   differs, `$18`, `$9`, `$28` and `$36` included, and `$13` (sets R) once (B2); treat `EEPROM read fail` as a factory
   reset: push the full set (blanks from their defaults), except a `$38` that `$$` shows already matches, and apply
   B2's restart if `$38` went out.
10. Feed model: modal F clamped to [1, `$15`]; G0 at `$15`; Z clamped to `$18 × |$19|`; G38.2 at 60 mm/min. Emit F
    on the first motion line; restart with a G1 Z plunge at an explicit feed, then restore the file's last F, if it set one
    (without one the plunge feed stays modal). Never lower before travel (owner decision, 2026-11-15): before the XY
    travel of any resume or restart, and of Home, Return to Center and triangular Accept, Z goes to max(safe height,
    the bit's Z) in the active units, and no Z move before that travel lowers the bit. The bit's Z is the Z
    WebControl last commanded, or, when a Stop, alarm, probe or reboot left it unknown, the last status report's MPos
    z plus one display step (0.01 in the report's units): MPos is only a lower bound for clearance here, so it is
    rounded up, never used to restore an exact Z (B15). A report read before the handshake's G20/G21, or while a line
    that moves Z or changes units is unacknowledged, is not used. With no report since then, the raise
    waits for one read after every line before it is acked (about every 200 ms, §2) and is refused with an alert
    after 3 s; it never guesses the safe height.
11. Never auto-send `~`; stop streaming before any line the firmware will pause on (using its T semantics); emit
    `T<n> M6`. After an ALARM or `Buffer overflow!`, Play resumes from the oldest unacknowledged line, returning at
    safe height (`alarm_mid_cut_rewinds_and_resumes`); Stop also clears it (owner decision, 2026-11-15).
12. Z limits: hide below 51.292 and never send B17/B18/B19; on 51.292+ use `$63`/`$64` (mm, shifted by zeroing)
    and B20 to read them.
13. Spindle on 51.29x: send `S` on its own line after M3. A resume or restart that restores `M3`/`M4` on 51.29x
    follows it with the file's last `S` (if it set one); 51.27 and stock get the bare `M3`, which does nothing with
    `$17=0`.
14. Kinematics: reproduce the firmware exactly (R rule, density, trig, tolerances, old-triangular branch, clamps,
    forward-solve limits). The live computed sled position uses the float32 triangular inverse
    (`wc-kinematics` `Firmware32`, checked against the same C built for float); holey calibration keeps Python's
    `f64` model so it matches the fixtures.
15. MPos has 2 decimals; never use it to restore an exact Z.
16. Touch Z: send `G38.2` alone; no `G10 Z0` after it; a probe `error:` alerts and zeroes nothing; until the probe
    answers, refuse every action that moves the machine (Resume included), except Stop. A `G38.2` streamed from a file
    is the same: stream nothing behind it, hold the job at the probe line on a miss, and take Z as 0 after a touch.
    An `ok` without `z axis zeroed` alerts that Z was not zeroed and pauses a job.

17. Alarm hold (2026-11-15): after `ALARM:`, `Buffer overflow!` or `Message: Unable to find valid machine position`
    (other than zero chain lengths), write nothing that can move the machine until the user clears it. Queued
    commands are dropped except the boot handshake, `$` lines and lone `G20`/`G21`/`G90`/`B05`; a job is held at its
    oldest unacknowledged line. Every action and HTTP write that reaches the controller is refused (an alert or a
    409) except Stop, Stop Z, Pause, Resume's `~`, `$$` and settings writes. Stop or Stop Z clears it, and so does
    Play (or Resume) on the held job, which returns as from any halt (B11). A reboot or reconnect keeps it, and B05's
    reply shows the alarm again instead of clearing it. The unplaceable-sled message that ends Stop's own reload is
    shown but does not hold again, however long the 200 failed guesses take. After it the firmware takes the sled to
    be at 0, 0, so a job it held resumes only from Stop. `alarm_mid_home_holds_the_plunge_until_stop`.

18. Lost lines (2026-11-15): on the owner's cut a line Rust wrote never reached the firmware (no echo, no `ok`,
    `[PE:]` space 127 and MPos unchanged for 44 s), and Rust waited for its `ok` until Stop; Python had the same
    blind spot. Unacknowledged lines are lost when, since the last ack, the controller has printed nothing but
    status reports (no echo, no message) and three reports in a row, each read at least 150 ms after the latest
    write, showed an empty ring (space 127), the same MPos and `Idle`, with no `~` awaiting its `ok`, no `Maslow
    Paused` or Z prompt, and no boot or resync (§1, §2: a line in the ring keeps space under 127, a line that runs
    has been echoed, a running move changes MPos). The firmware reads the port into the ring before every report
    (§2) and a line is under 10 ms on the wire at 57600 baud, so a report made after the bytes arrived shows space
    under 127 or follows the echo; reports within 150 ms of the write are ignored all the same, and three are
    needed so one garbled or glued report is never enough. Detection comes about 0.6–0.8 s after the write. On
    the owner's two cuts (3247 lines sent) 51.27 echoed or answered every line it received within 40 ms (about
    20 ms typically), and the rule finds all 11 lost lines and no other. They were never run, so each goes out
    again once, as written, with a controller message naming it; the 126-byte accounting forgets the lost ones first. A job line
    lost again pauses the job at it (`Line Not Received`) and Resume returns to the file there and sends it; a
    command lost again is dropped with the commands queued after it, `G20`/`G21` and `G90` are written to put
    the firmware back in step, and a job in progress pauses. A job the user paused waits at the lost line for
    Resume instead. A line written while another awaited its `ok` (buffered mode) is never taken for lost: with
    an earlier line lost, the next line's echo and `ok` would be counted for it. Not covered: a line whose `\n` alone was lost (the ring keeps its bytes, space stays under
    127) and a line that ran while both its echo and `ok` were lost on the way back.
    `stream_lost_line_is_sent_again`, `stream_line_lost_twice_pauses_until_resume`.

19. Arcs as the file means them (upstream #120), on every firmware version: every line as it is written (a job
    line after the home shift, a typed line, a restart's) has each G2/G3 segment with an R and no I or J rewritten
    with I and J at 4 decimals, R removed, and each G2/G3 segment under a real G91 rewritten with the absolute X and
    Y it ends at (start plus offset, 4 decimals; I and J are already offsets and stay as written). The centre is the
    standard one: G2 clockwise, G3 counter-clockwise, a negative R the arc over 180°. 51.29x gets the same bytes as
    51.27: it reads R but drops its sign (§9), and one path keeps what is cut independent of the version. The start
    is where WebControl last commanded the sled, following the controller's own parse (segments split at G and M,
    the modal G, G20/G21, G90/G91 with G91.1 truncated to G91 for G0/G1; G2/G3 X/Y absolute); after a Stop, an
    alarm, a line lost twice (B18), a `B` or `$` line, or with no absolute move yet, it is the last status report's MPos read while nothing
    that moves is unacknowledged. G91.1 and G90.1 (arc-centre modes) do not make an arc relative. Refused like an
    unclosed comment (a job pauses before the line with an alert; a typed line is dropped with one): |R| shorter
    than half the chord by over 0.05 mm, an R arc with no X/Y change, an R arc under G18/G19, a G91 arc with a Z
    word (the firmware would take it as absolute), or no known start where one is needed. The rewritten line must
    still fit B3. Since what is sent then ends where the file means, the file-side position and restart state keep
    reading G91 arcs as relative; the commanded Z (B10) reads an arc's Z as absolute, as the firmware does. The
    preview draws R arcs with the same centre and real-G91 arcs relative. `r_arc_converted_to_i_j_on_51_27`,
    `r_arc_impossible_radius_pauses_the_job`, `r_arc_converted_on_51_292`, `g91_arc_written_absolute`,
    `g91_arc_with_z_pauses_the_job`.

### C. One-click firmware upgrade

1. **Pre-flight:** machine idle; capture the boot banner, B05, `$$` and the B10 chain lengths. Block (or require
   explicit acknowledgement) for a `PCB v1.6 TLE9201` banner, TLE5206 or an unknown board; warn about 51.292's
   TLE9201 left-encoder swap.
2. **Backup:** the `$$` dump, WebControl's settings, the chain lengths, and the current hex for rollback.
3. **Artefact:** a pinned, SHA-256-checked application hex (never `with_bootloader`); 51.291's shipped hex; prefer
   the v51.292 tag.
4. **Flash:** `!`, close the port, block reconnects, run avrdude with verify, retry once on a sync failure.
5. **Post-flash:** the banner's driver and PCB must match pre-flight (else roll back); B05 must report `51.29`;
   expect `EEPROM read fail`.
6. **Settings migration:** read `$$`, then push the complete set, including `$18` = 12.6 for the stock Z motor,
   `$9`, `$28`, `$36` and an explicit `$17`; set `$63`/`$64` on 51.292+; never send `$60`/`$61` on 51.291; skip a
   `$38` the `$$` shows already held. A `$38` written here (the factory value is 1) restarts the controller as B2
   says; the push after that boot skips it and verifies with `$$`; re-send units and G90.
7. **Self-test:** with the chains slack, B04 must pass both directions on L, R and Z; re-zero Z; compare B10 chain
   lengths with pre-flight and guide B08/B02 if they differ.
8. **Rollback:** reflash the saved hex, then push the full settings again (downgrades reset too).
9. The parser must tolerate the 51.29x text quirks before any post-flash step runs.
