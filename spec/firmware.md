# Maslow firmware vs WebControl backend

Findings from reading the firmware source, 2026-11-14. This is the reference the Rust backend and the firmware
upgrade flow are built against. Firmware paths are relative to `cnc_ctrl_v1/`.

| Tag | Repo @ commit | Notes |
|---|---|---|
| S126 | MaslowCNC/Firmware @ e1e0d02 | Stock 1.26 |
| H27 | madgrizzle/Firmware @ 950fb23 | Holey 51.27, bundled by `buildFirmware.sh`; the user's controller |
| H28 | madgrizzle/Firmware @ e9cc445 | 51.28: FAKE_SERVO handling and version only |
| H291 | Orob-Maslow/Firmware @ 97d12a6 | Tags v51.29 / v51.291 (master); reports `51.29` |
| H292 | Orob-Maslow/Firmware @ 0064955 | Tag v51.292; the planned upgrade target |
| H293 | Orob-Maslow/Firmware @ 300f6c7 | Head of 291_testing; reports 51.293, differs in Z-limit defaults, encoder pins, B04 |

## 1. Serial protocol

- **Link:** 57600 baud 8N1 in every version. Lines in end at `\n` (`\r` is stripped); lines out end `\r\n`.
- **Buffer:** `INCBUFFERLENGTH 128`, a ring buffer holding 127 bytes. WebControl's `bufferSize = 126` is safe; it
  counts the appended space and `\n` exactly as stored. `?` and the realtime characters never enter the ring.
- **Over-long lines:** a line over 126 characters overflows the ring. The firmware prints `Buffer overflow!`, stops,
  flushes, and then **executes the unread tail of the line as a new command** and acks it.
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
`space` is the firmware's true free space. B04 prints a status line with no newline.

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

## 5. Feed rate on Z moves

F is per minute in current units, stored as mm/min (×25.4 under G20), clamped to [1, `$15`], and modal (G0 sets it
too). G0 runs at `$15` and ignores F for speed. Z is clamped to `zMaxFeed = $18 × |$19|`: 39.9 mm/min on the 51.27
defaults, 399 mm/min on the 51.292 defaults. WebControl's restart sequence rapids without F, plunges with
`G1 Z… F<plunge>` (the file's last non-G0 Z feed, else 12.8 mm/min) and ends with `G0`/`G1 F<file F>` to restore the
file's modal motion and feed. If the file set no F before that point it restores no F (only `G0`/`G1` where the
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
  selected, stops streaming after a tool-changing M6, and on Resume returns to the file's XY at safe height.
- Manual Z (`$16=0`): `Message: Please adjust Z-Axis…` then pause; handled correctly.
- Alarms: "Sled not keeping up" stops the firmware with no acks; "Board invalid" hangs it.

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
an S line). R-format arcs. `$60`/`$61` spindle min/max; `$63`/`$64` Z limits (51.292). TB6643 boards. Text quirks:
` prescaler to ` appended to the boot line, `the value received:` after each `$` write, `ok` glued to a line after
`$60`/`$61` on 51.291, `Change in setting, rewriting EEPROM` on 51.292. Comments no longer hang the firmware (51.27
loops forever on one, so WebControl's comment stripping is load-bearing).

## Implications

### A. Current Python backend

| # | Severity | Finding |
|---|---|---|
| A1 | High | Lines over 126 characters are sent "alone" when idle; the firmware overflows and executes the line's tail. |
| A2 | Fixed | `$18`, `$9`, `$28`, `$36` were never synced; now adopted from `$$` and synced, and an EEPROM reset triggers a full restore. Install WebControl before flashing 51.292, or the 51.292 default `$18=126` could be adopted. |
| A3 | Fixed | Tool change: `~` was auto-sent; `T`/`M6` on separate lines ran on without a tool change; `M6 T2` stalled; a race could make Resume a no-op. |
| A4 | Fixed | Restart/resume plunged with G0 (at zMaxFeed) and left the modal F at the XY feed. |
| A5 | Medium | Z-limit commands do nothing on 51.27/51.28, overwrite `$43`/`$44` on 51.291, store nothing on 51.292. |
| A6 | Medium | When the firmware stops itself (ALARM, overflow), acks are lost and streaming blocks until Stop. |
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

### B. Rust backend requirements

1. 57600 8N1; send `\n`, parse `\r\n`; don't gate the first send only on the boot `ok`.
2. Treat any line containing `PCB v1.`, `Grbl v1.00` or `ready` as a boot. On each boot reset units, G90, tool and
   accounting, then B05 → units → G90 → full settings push → `$$` verify.
3. Character-count flow control against 126 bytes; free on `ok`/`error:`; hard-reject lines over 126 characters;
   optionally resync from the PE `space` field when idle.
4. Realtime: `!` resets accounting (immediate `ok`, no ack for the aborted line); `~` immediate `ok`.
5. Sanitise outgoing lines: strip comments, reject an unclosed `(`, remove `!`, `~`, backtick, `?` and control
   characters.
6. ALARM or `Buffer overflow!` halts streaming and rewinds the job to the oldest unacknowledged line; acks are
   ignored until a status report (the resync); a probe `error:` is that line's ack;
   `$RST` has no `ok` of its own; detect a trailing glued `ok` on 51.291.
7. Parse every message type above, including B04's glued status line.
8. B05 after every boot; parse `^Firmware Version\s+([\d.]+)`; fingerprint 51.29x; record the PCB/driver banner.
9. Settings: per-version key/type table; compare at the printed precision; push the full set after every boot,
   including `$13` (sets R) and `$18`, `$9`, `$28`, `$36`; treat `EEPROM read fail` as a factory reset.
10. Feed model: modal F clamped to [1, `$15`]; G0 at `$15`; Z clamped to `$18 × |$19|`; G38.2 at 60 mm/min. Emit F
    on the first motion line; restart with a G1 Z plunge at an explicit feed, then restore the file's last F, if it set one
    (without one the plunge feed stays modal).
11. Never auto-send `~`; stop streaming before any line the firmware will pause on (using its T semantics); emit
    `T<n> M6`. After an ALARM or `Buffer overflow!`, Play resumes from the oldest unacknowledged line, returning at
    safe height (`alarm_mid_cut_rewinds_and_resumes`); Stop also clears it (owner decision, 2026-11-15).
12. Z limits: hide below 51.292 and never send B17/B18/B19; on 51.292+ use `$63`/`$64` (mm, shifted by zeroing)
    and B20 to read them.
13. Spindle on 51.29x: send `S` on its own line after M3.
14. Kinematics: reproduce the firmware exactly (R rule, density, trig, tolerances, old-triangular branch, clamps,
    forward-solve limits).
15. MPos has 2 decimals; never use it to restore an exact Z.
16. Touch Z: send `G38.2` alone; no `G10 Z0` after it; a probe `error:` alerts and zeroes nothing; until the probe
    answers, refuse every action that moves the machine (Resume included), except Stop. A `G38.2` streamed from a file
    is the same: stream nothing behind it, hold the job at the probe line on a miss, and take Z as 0 after a touch.
    An `ok` without `z axis zeroed` alerts that Z was not zeroed and pauses a job.

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
6. **Settings migration:** push the complete set, including `$18` = 12.6 for the stock Z motor, `$9`, `$28`, `$36`
   and an explicit `$17`; set `$63`/`$64` on 51.292+; never send `$60`/`$61` on 51.291; verify with `$$`; re-send
   units and G90.
7. **Self-test:** with the chains slack, B04 must pass both directions on L, R and Z; re-zero Z; compare B10 chain
   lengths with pre-flight and guide B08/B02 if they differ.
8. **Rollback:** reflash the saved hex, then push the full settings again (downgrades reset too).
9. The parser must tolerate the 51.29x text quirks before any post-flash step runs.
