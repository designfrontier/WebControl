# Jobs: an air cut, then a real cut

What this must show on the real machine: `webcontrol-rs` runs a job: Play, Pause, Resume and Stop; an M0 and a tool
change in the file; a jog and a Z move while held; a restart mid-file; the job lock and the other refusals; and then
**one real cut**. CI replays every transcript in `contract/serial/transcripts/` against the machine core, checks each
action's refusals in every run state, and runs a simulated air cut with the same steps as section 3
(`wc-machine/tests/streaming.rs`). GPIO buttons and LEDs are in section 6, only if you have any wired.

**This checklist moves the sled with a job. Read every safety line.**

Safety, for every step below (and the README's):

- Sections 2 to 6 are an **air cut**: the router is switched off at its own switch, the bit is removed, and Z is
  zeroed 30 mm above the work, so every depth in the file stays in the air.
- Keep a hand near **Stop**, and know where the controller's power switch is. If Stop does not stop the sled within a
  second, cut the controller's power, then report it before going on.
- Section 7, the real cut, comes only after every box in sections 2 to 4 is ticked.

You need: the release installed, the controller on USB (holey 51.27 or 51.292) and connected, the machine calibrated,
the log viewer open beside the control screen, and a browser.

## 1. Before you start

- [ ] Note Settings ▸ Maslow ▸ Z-Axis Safe Travel Height in MM (5 mm by default). The checks below call it `<safe>`.
- [ ] Make the air-cut file on the Mac: a 40 mm square at X/Y 0–40 around home, an M0, a tool change to tool 2, then
      a second square.

      cat > ~/aircut.nc <<'EOF'
      (air cut for the WebControl jobs checklist)
      G21
      G90
      G0 Z5
      G0 X0 Y0
      G1 Z-3 F100
      G1 X40 F600
      G1 Y40
      M0 (pause here: press Resume)
      G1 X0
      G1 Y0
      G0 Z5
      T2 M6
      G0 X10 Y10
      G1 Z-2 F100
      G1 X30 F600
      G1 Y30
      G1 X10
      G1 Y10
      G0 Z5
      G0 X0 Y0
      EOF

## 2. Set up the air cut

- [ ] **Router off at its switch, bit out.** Jog the sled to the middle of the bed and press Set Home Here.
- [ ] Z-Axis (beside the jog pad): raise Z 30 mm, then **Set Z Zero Here**. Every Z in the file is now in the air.
- [ ] G-Code ▸ Upload `~/aircut.nc` and open it. The preview shows the two squares around home, and the run panel
      shows "File: aircut.nc".

## 3. The air cut

Watch the sled and the log viewer side by side. The lines after each press are the ones the named transcript pins.

- [ ] **Play.** The log shows `Sent: G21`, `G90`, `G0 Z5`, then the moves, one per `ok`. The sled rises, moves to
      home, "plunges" (in the air) and traces the first two sides. The Play LED (if wired) lights.
- [ ] **M0.** After `Sent: M0` the controller prints `Maslow Paused`, the sled stops, the Pause button reads Resume,
      and nothing more is sent. No `~` goes out until you press a button.
- [ ] While held at the M0: jog right 10 mm. The log shows `Sent: ~` once, then `G91`, `G00 X10.0`, `G90`, and the
      sled moves 10 mm.
- [ ] **Resume.** The log shows `Sent: ~`, then the return: `G21`, `G90`, `G0 Z5.0`, `G0 X40.0 Y40.0` (plus home),
      `G1 Z-3.0 F100.0`, `G1 F600.0`, `G90`, then `G1 X0` (`m0_in_file_then_resume`). The sled rises, returns to the
      corner where it paused, lowers at the plunge feed, and carries on.
- [ ] **Tool change.** After `Sent: T2 M6` the controller prints `Tool Change: Please insert tool 2` and
      `Maslow Paused`; the Job readout says "Waiting for tool change", the prompt shows with a Resume button, and the
      sled stays at the safe height.
- [ ] While the tool change waits: Z-Axis, raise 10 mm. The log shows `Sent: ~` once, then the Z move. Raise 10 mm
      again: no second `~`. Press **Set Z Zero Here**: `Sent: G10 Z0` (`tool_change_jog_and_zero_z_then_resume`). For
      the air cut this re-zeroes Z 50 mm up; that is fine.
- [ ] **Resume.** `Sent: ~`, then `G21`, `G90`, `G0 Z5.0`, the XY where the M6 was, `G0 F600.0`, `G90`, and the second
      square starts with the file's own `G0 X10 Y10` and `G1 Z-2 F100`. The bit is never lowered before the file's own
      Z move.
- [ ] **Pause** mid-square. The line in flight finishes, then nothing more is sent. The Pause LED lights.
- [ ] While paused, Home: the sled goes home at the safe height and lowers to Z 0 (in the air, 50 mm up). Then
      **Resume**: it returns to where it paused at the safe height, lowers to the depth it paused at
      (`G1 Z-2.0 F100.0` relative to your new zero), and carries on (`pause_jog_resume_returns_to_file_position`).
- [ ] **Never lower before travel.** While the second square runs, **Pause** mid-side (the bit is at the file's Z-2),
      then raise Z with Z-Axis until it is above the safe height, for example `<safe>` + 10 mm. Note the Z readout.
      **Resume**: after `G21` and `G90` the log shows `G0 Z<that Z>` (to 0.01 mm, not `G0 Z<safe>`), then the XY
      move. The bit never dips before the sled travels, and only then lowers to `G1 Z-2.0 F100.0`
      (`resume_above_safe_height_never_lowers_before_travel`). Pause again, raise Z the same way, then right-click
      (long-press on a tablet) the viewer and press Return to center: the log shows `G00 Z<that Z>` before
      `G00 X0.0 Y0.0`, and the bit does not dip. **Resume**: the job carries on.
- [ ] **Stop** mid-square. The log shows `Sent: !` at once, the sled stops within about a second, the job ends (the
      index goes back to line 1), the Stop LED lights, and the next manual command goes out only after a status
      report (about 0.3 s). `journalctl -u webcontrol-rs` may say "a Stop cut short the line being written" or "lines
      queued behind a Stop were never written": both are expected, and the controller flushed those lines anyway.
- [ ] Settings ▸ WebControl ▸ Computed Sled Position on, then Play and **Stop** mid-square again: `Sent: !` still
      comes at once. Turn the setting back to what it was.
- [ ] **Restart mid-file.** Move the G-code index to the `G1 X30 F600` line of the second square, then **Play**. The
      log shows `G21`, `G90`, `G0 Z5.0`, `G0` to the XY of `G0 X10 Y10` plus home, `T2 M6` (the controller already has
      tool 2, so it does not pause), `G1 Z-2.0 F100.0`, `G1 F600.0`, `G90`, then the rest of the file from `G1 X30`
      (`start_mid_file_restores_modal_state`). The job ends at home with Z at 5.
- [ ] Throughout: no `Buffer overflow!`, no alarm, and no line was sent twice or skipped (compare the log's `Sent:`
      lines with the file). A line the controller lost is sent again with a controller message: see `pacing.md`.

## 4. The job lock and other refusals

- [ ] While a job streams, the jog arrows, Home, the macros, the Z-Axis dialog's Raise, Lower and Set Z Zero Here,
      Set Home Here and the Units switch are disabled, and so is Settings Save; Stop and Pause still work.
- [ ] While paused, G-Code ▸ Open, Upload and Clear GCode are greyed out with "Stop the job first", and Set Home Here
      stays disabled until Stop.
- [ ] While paused: Settings ▸ Maslow shows "A job is running or paused. Stop the job first to change the settings."
      and Save stays disabled even after changing Serial Connection; the jog panel's Units switch is disabled; G-Code ▸
      Send Custom GCode and the imports are disabled with the same kind of note, and Actions ▸ Restore WebControl…
      is disabled.
- [ ] From the Mac, while paused, a settings write and a send are refused and nothing reaches the log:

  ```sh
  curl -s -w ' %{http_code}\n' -X PUT -H 'Content-Type: application/json' \
    -d '{"values":{"COMport":"/dev/ttyUSB0"}}' http://maslow.local:5000/api/settings/maslow
  curl -s -w ' %{http_code}\n' -F gcode='G0 X5' http://maslow.local:5000/sendGCode
  ```

  The first ends `409` after "Cannot change settings while a job is running or paused. Stop the job first.", the
  second `409` after "Cannot send gcode while a job is running or paused. Stop the job first.". The log shows no
  `Sent: G0 X5`, and the port stays connected. Resume finishes the square. After Stop, Save works again.
- [ ] The display theme and the line pacing are not locked: both change while paused (`ui.md` section 3,
      `pacing.md` section 2).
- [ ] Make a file with an unclosed comment on line 3 (`G1 X5 (rough`) and Play it: the job pauses before line 3 with
      the alert "Line Unclosed Comment", and line 3 is never sent (`unclosed_comment_pauses_the_job`). Stop.
- [ ] Only with a touch plate set up, its clip on the collet (router off, bit still out): make `~/probe.nc` with the
      lines `G21`, `G90`, `G0 Z5`, `G38.2 Z-10 F100`, `G0 Z5`, `G0 X10` and Play it, holding the plate in your hand.
      The Job readout says Probing, and nothing after the `G38.2` is sent until you touch the plate to the collet;
      then Z reads 0 and the job finishes (`probe_in_file_waits_for_the_touch`). Play it again and let the probe run
      out: the alert "Touch Z Failed" shows, the job stays paused on the `G38.2` line, and **Resume** raises Z,
      returns and probes again (`probe_in_file_misses_and_waits_at_the_probe`). Stop.
- [ ] Optional, since a controller reset can lose the chain lengths: with the sled moving in a job, unplug the USB
      cable for 6 s and plug it back in. The UI shows "Connection Lost"; the job stays paused (Resume shown); the
      controller reboots and its handshake (`$$`, then `$13`) goes out. Press **Resume**: the sled returns at the safe
      height to the first line the controller had not finished, lowers, and carries on.

## 5. Buffered mode (optional; off by default)

- [ ] Settings ▸ Maslow ▸ Buffer Gcode on, then unplug the controller's USB for 6 s and plug it back in (WebControl
      reads Buffer Gcode when the port opens). Air-cut `~/aircut.nc` again from Play. Several lines go out before each
      `ok`, the status bar's "Buffer <n>" chip (free space, 127 when empty) drops below 127, there is no
      `Buffer overflow!`, and the M0 and tool change still stop exactly where they did. Turn Buffer Gcode back off and
      replug the USB.

## 6. GPIO (only if buttons or LEDs are wired)

- [ ] Settings ▸ GPIO shows your pins. `journalctl -u webcontrol-rs` has no "no GPIO here" line (that would mean the
      header could not be opened).
- [ ] A Play button starts the air cut; a Pause button pauses; Play again resumes; Stop stops at once. Each LED
      follows: Play on at start, Pause on while paused, Stop on after Stop.
- [ ] A button pressed while the job streams does what the UI would: Go Home alerts "Cannot issue command while
      sending gcode." and nothing moves. A bouncing button does not trigger twice.
- [ ] Status LED: wire an LED and resistor from 3.3 V to a free pin (active low) and set that pin to `StatusLED Active
      Low`, then restart WebControl. With the USB unplugged it double-blinks; plugged in and idle it is steady on.
      During the air cut it blinks slowly (1 s on, 1 s off); Pause, an M0 and a tool change blink it fast (4 times a
      second); unplugging the USB mid-job double-blinks it. An alarm (for example the sled held back until "Sled Not
      Keeping Up") flickers it until Stop. Play, Pause and Stop LEDs, if wired, still follow as above.
- [ ] `sudo systemctl stop webcontrol-rs` turns the status LED off and it stays off. Rewire it from the pin to ground
      (active high), set the pin to `StatusLED`, restart: the same patterns show, and stopping turns it off.

## 7. The first real cut

Only after every box in sections 2 to 4 is ticked. Use scrap you do not mind ruining, clamped flat, with the sled
clear of every clamp.

- [ ] Make a shallow test file on the Mac: a 50 mm square outline, 1 mm deep.

      cat > ~/firstcut.nc <<'EOF'
      (first real cut: 50 mm square, 1 mm deep)
      G21
      G90
      G0 Z5
      G0 X0 Y0
      G1 Z-1 F100
      G1 X50 F600
      G1 Y50
      G1 X0
      G1 Y0
      G0 Z5
      G0 X0 Y0
      EOF

- [ ] Fit the bit. Jog over the scrap, press Set Home Here, and zero Z on the surface (Z-Axis ▸ Touch Plate, or
      lower by hand and Set Z Zero Here). Raise Z 5 mm.
- [ ] Open `~/firstcut.nc`; the preview shows the square on the board. **Router on.** Play.
- [ ] Pause once mid-side and Resume: the bit lifts, returns, lowers to 1 mm at the plunge feed, and carries on.
- [ ] The cut is a closed 50 mm square, 1 mm deep, with no gouge at the pause point, no double cut and no missing
      segment. Measure it: ______ mm by ______ mm.
- [ ] Router off. The run panel's pacing stats show the job's lines per minute and lost lines: note them for
      `pacing.md`.

Report as the README says.

## Where this differs from v0.94

For testers who know WebControl v0.94; `contract/serial/README.md` ("Rust-intended deviations") has the full list.

- **A `$38` write resets the controller** (`settings.md` section 2).
- **Stop Z** (in the Z-Axis dialog) ends a running or paused job too.
- **A lost link** pauses the job at the first line the controller had not acknowledged, not at the next one WebControl
  had not sent.
- **A lost line** is sent again once, and a line lost twice pauses the job at it (`pacing.md` section 1). v0.94 waited
  for its `ok` until Stop.
- **A controller reboot while streaming** stops the job with an alert; a reboot while the job is paused, as after a
  reconnect, keeps it paused.
- **Stop forgets the units a job paused in**, so a later job's Resume never switches units back.
- **Queued commands** (jogs, Send Custom GCode, macros) do not resend the G-code position after each line; streamed
  lines do.
- **The job lock:** while a job is running or paused, or a tool change waits, settings saves (the serial port too),
  Quick configure, the unit switches, the imports, Restore and Send Custom GCode are refused.
- **Never lower before travel:** every resume and restart, Home, Return to center and triangular Accept rise to the
  higher of the safe height and the bit's own height before any XY travel.
- **51.29x spindle speed:** on 51.292, a resume that restores `M3`/`M4` follows it with the file's last `S` on its own
  line, since 51.29x leaves the PWM at 0 until an `S` line (`resume_on_51_292_sets_the_spindle_speed_after_m3`). On
  51.27 nothing changes.
- **A `G38.2` in the file** is tracked like Touch Z: streaming waits behind it, moves are refused, a miss holds the job
  at the probe line, and a probe that ends without `z axis zeroed` alerts.
- **R arcs and G91 arcs** are written as the firmware reads them: R becomes I and J, and a G91 arc's end is written
  absolute (`r_arc_converted_to_i_j_on_51_27`, `g91_arc_written_absolute`).
