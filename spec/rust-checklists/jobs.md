# M4 hardware checklist: jobs on the real machine

What M4 must show on the real machine: `webcontrol-rs` runs a job: Play, Pause, Resume and Stop; a restart mid-file;
an M0 and a tool change in the file; a jog and a Z move while paused; and then **one real cut**. In CI every
transcript in `contract/serial/transcripts/` replays byte for byte against the machine core, every action is
refused in exactly the run states its gate lists with each
state reached through `wc-sim`, and a simulated air cut with the same steps as section 4 passes. GPIO buttons and LEDs
are ported too (section 7, only if you have any wired).

**This milestone moves the sled with a job. Read every safety line.**

Safety, for every step below:

- Sections 3 to 6 are an **air cut**: the router is switched off at its own switch, the bit is removed, and Z is
  zeroed 30 mm above the work, so every depth in the file stays in the air.
- Keep a hand near the UI's **Stop** button, and know where the controller's power switch is. If Stop ever fails to
  stop the sled within a second, cut the controller's power, then report it before going on.
- Section 8, the real cut, comes only after every box in sections 3 to 6 is ticked.

You need: the Mac with this branch checked out, `pip install ziglang` done once, the Pi booted from the new microSD card
(`pi@maslow.local`), the controller on USB (holey 51.27 on PCB v1.1), the Python service installed and working,
the machine calibrated, and a browser. Tick each box; if one fails, note what you saw, press Stop, and run
`wc-switch python`.

## 1. CI

- [ ] `cd rust && cargo test --workspace` passes, including `wc-machine`'s `transcripts` (every transcript),
      `gates`, `jobs`, `tool_change`, `clearance`, `streaming` (with the simulated air cut) and `gpio` tests.
- [ ] `rust/scripts/coverage.sh` reports every crate at or above 95%.
- [ ] `cd rust && cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.36 -p wc-server` builds.

## 2. Before you start

- [ ] On Python, as usual: connect, check "Firmware: Holey 51.27", then close the browser tab.
- [ ] Back up the settings (never edit the original): `scp -r pi@maslow.local:.WebControl ~/webcontrol-backup-m4/`.
- [ ] Note the time for the journal: `date`. Note Settings → Maslow Settings → Z-Axis Safe Height (5 mm by default; the owner's
      machine uses 10 mm). The checks below say `<safe>` for it.
- [ ] Make the air-cut file on the Mac (40 mm square at X/Y 0–40 around home, an M0, a tool change to tool 2, then a
      second square):

      cat > ~/aircut.nc <<'EOF'
      (air cut for webcontrol-rs M4)
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

## 3. Install, switch, and set up the air cut

- [ ] From the Mac (repo root): `rust/deploy.sh install` prints `Installed webcontrol-rs 0.1.0 (<commit>, contract 3)
      for pi.` On the Pi: `wc-switch rust`.
- [ ] Open `http://maslow.local:5000/`. The controller connects, "Firmware: Holey 51.27" shows, and the log
      viewer (`/logs`) shows the boot push and `$$` as in M2.
- [ ] **Router off at its switch, bit out.** Jog the sled to the middle of the bed. Define Home there.
- [ ] Z panel: raise Z 30 mm, then **Define Z zero** there. Every Z in the file is now in the air.
- [ ] G-Code → Upload `~/aircut.nc` and open it. The preview shows the two squares around home.

## 4. The air cut (the M4 "done when", part 1)

Watch the sled and the log viewer side by side.

- [ ] **Play.** The log shows `Sent: G21`, `G90`, `G0 Z5`, then the moves one per `ok`. The sled rises, moves to home,
      "plunges" (in the air) and traces the first two sides. The Play LED (if wired) lights.
- [ ] **M0.** After `Sent: M0` the controller prints `Maslow Paused`, the sled stops, the Pause button reads Resume,
      and nothing more is sent. No `~` is sent until you press a button.
- [ ] While held at the M0: jog right 10 mm. The log shows `Sent: ~` once, then `G91`, `G00 X10.0`, `G90`, and the sled
      moves 10 mm.
- [ ] **Resume.** The log shows `Sent: ~`, then the return: `G21`, `G90`, `G0 Z5.0`, `G0 X40.0 Y40.0` (with home added),
      `G1 Z-3.0 F100.0`, `G1 F600.0`, `G90`, then `G1 X0`. The sled rises, returns to the corner where it paused,
      lowers at the slow plunge feed, and carries on.
- [ ] **Tool change.** After `Sent: T2 M6` the controller prints `Tool Change: Please insert tool 2` and `Maslow
      Paused`; the UI shows the prompt with a Resume button; the sled stays at safe height.
- [ ] While the tool change waits: Z panel, raise 10 mm. The log shows `Sent: ~` once, then the Z move; raise 10 mm
      again and no second `~` goes out. Press **Define Z zero**: `Sent: G10 Z0`. (For the air cut this re-zeroes Z 50 mm
      up; that is fine.)
- [ ] **Resume.** `Sent: ~`, then `G21`, `G90`, `G0 Z5.0`, the XY where the M6 was, `G0 F600.0`, `G90`, and the second
      square starts with the file's own `G0 X10 Y10` and `G1 Z-2 F100`. The bit is never lowered before the file's own
      Z move.
- [ ] **Pause** mid-square. The line in flight finishes, then nothing more is sent. The Pause LED lights.
- [ ] While paused, Actions → Home: the sled goes home at safe height and lowers to Z 0 (in the air, 50 mm up).
      Then **Resume**: it returns to where it paused at
      safe height, lowers to the depth it paused at (`G1 Z-2.0 F100.0` relative to your new zero), and carries on.
- [ ] **Never lower before travel** (owner decision, 2026-11-15). While the second square still runs, **Pause**
      mid-side (the bit is at the file's Z-2), then Z panel: raise Z until the bit is above the safe height, e.g.
      `<safe>` + 10 mm. Note the Z readout. **Resume**: after `G21` and `G90` the log shows `G0 Z<that Z>` (to 0.01 mm; not
      `G0 Z<safe>`), then the XY move. Watch the bit: it never dips before the sled travels, and only then lowers to
      `G1 Z-2.0 F100.0`. Pause again, raise Z the same way, then Actions → Return to Center: the log shows
      `G00 Z<that Z>` before `G00 X0.0 Y0.0` and the bit does not dip. **Resume**: the job carries on.
- [ ] **Stop** mid-square. The log shows `Sent: !` at once, the sled stops within about a second, the job ends (the
      index goes back to line 1), the Stop LED lights, and the next manual command goes out only after a status
      report (about 0.3 s). `journalctl -u webcontrol-rs` may say "a Stop cut short the line being written" or "lines
      queued behind a Stop were never written": both are expected, and the controller flushed those lines anyway.
- [ ] Settings → WebControl Settings → Computed Sled Position on, then Play and **Stop** mid-square again: `Sent: !` still
      comes at once (each status report's position is solved off the machine task). Turn it back to what it was.
- [ ] **Restart mid-file.** Move the gcode index to the `G1 X30 F600` line of the second square, then **Play**. The
      log shows `G21`, `G90`, `G0 Z5.0`, `G0` to the XY of `G0 X10 Y10` plus home, `T2 M6` (the controller already has
      tool 2, so it does not pause), `G1 Z-2.0 F100.0`, `G1 F600.0`, `G90`, then the rest of the file from
      `G1 X30`. The job ends at home with Z at 5.
- [ ] Throughout: no `Buffer overflow!`, no alarm, and no line was sent twice or skipped (compare the log's `Sent:`
      lines with the file).

## 5. Refusals and the controller's own pauses

- [ ] While a job streams, every jog, home and setting button alerts "Cannot issue command while sending gcode." and
      sends nothing; Stop, Pause and Resume still work.
- [ ] While paused, G-Code → Open another file: it is refused ("Cannot load gcode while a job is running or paused.
      Stop the job first."). Define Home is refused until Stop.
- [ ] While paused (owner decision 2): Settings → Maslow Settings shows "A job is running or paused. Stop the job
      first to change the settings." and Save stays disabled even after changing Serial Connection; the jog panel's
      Units switch is disabled; G-Code → Send G-code, Settings → Import and Actions → Restore WebControl… are disabled
      with the same kind of note. From the Mac, a settings write and a send are refused and nothing reaches the log:
      `curl -s -X PUT -H 'Content-Type: application/json' -d '{"values":{"COMport":"/dev/ttyUSB0"}}'
      http://maslow.local:5000/api/settings/maslow` answers `409` with "Cannot change settings while a job is
      running or paused. Stop the job first.", and `curl -s -F gcode='G0 X5' http://maslow.local:5000/sendGCode`
      answers `409` with "Cannot send gcode while a job is running or paused. Stop the job first."; the log shows no
      `Sent: G0 X5` and the port stays connected. Resume finishes the square as before. After Stop, Save works again.
- [ ] Open a file, then press Play **immediately** while the preview is still drawing (a big file helps): Play alerts
      "Gcode is still loading" and nothing moves. Press Play again once the preview is there.
- [ ] Make a file with an unclosed comment on line 3 (`G1 X5 (rough`), Play it: the job pauses before line 3 with the
      alert "Line Unclosed Comment", and line 3 is never sent. Stop.
- [ ] Optional, since a controller reset can lose the chain lengths: with the sled moving in a job, unplug the USB
      cable for 6 s and plug it back in. The UI shows "Connection Lost";
      the job stays paused (Resume shown), the controller reboots and its handshake (`$$`, then `$13`) goes out. Press
      **Resume**: the sled returns at safe height to the first line the controller had not finished, lowers, and
      carries on. (Python resumed at the next unsent line instead; Rust goes back to the first one it lost.)

- [ ] Only with a touch plate set up and clipped to the bit: make `~/probe.nc` with the lines `G21`, `G90`, `G0 Z5`,
      `G38.2 Z-10 F100`, `G0 Z5`, `G0 X10` and Play it (air-cut setup, holding the plate in your hand). The job
      readout says Probing and nothing after the `G38.2` is sent until you touch the plate to the bit; then Z reads 0
      and the job finishes. Play it again and let the probe run out without touching: the alert "Touch Z Failed"
      shows, the job stays paused on the `G38.2` line (Resume shown), and Play raises Z, returns and probes again.
      Stop.

## 6. Buffered mode (optional; Python's default is off)

- [ ] Settings → Maslow Settings → Buffer GCode on, reconnect (power-cycle the controller), and air-cut `~/aircut.nc`
      again from Play. Several lines go out before each `ok`, the health bar shows the buffer filling, there is no
      `Buffer overflow!`, and the M0 and tool change still stop exactly where they did. Turn Buffer GCode back off.

## 7. GPIO (only if buttons or LEDs are wired)

- [ ] Settings → GPIO Settings shows the same pins as on Python. `journalctl -u webcontrol-rs` has no "no GPIO here"
      line (that would mean rppal could not open the header).
- [ ] A Play button starts the air cut; a Pause button pauses; Play again resumes; Stop stops at once. Each LED follows:
      Play on at start, Pause on while paused, Stop on after Stop.
- [ ] A button pressed while the job streams does what the UI would: Go Home alerts "Cannot issue command while
      sending gcode." and nothing moves. A bouncing button does not trigger twice.

## 8. The first real cut (the M4 "done when", part 2)

Only after every box above is ticked. Use scrap you do not mind ruining, clamped flat, with the sled clear of every
clamp.

- [ ] Make a shallow test file on the Mac: a 50 mm square pocket outline 1 mm deep:

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

- [ ] Fit the bit. Jog over the scrap, Define Home there, and zero Z on the surface (Touch Z with the plate, or lower
      by hand and Define Z zero). Raise Z 5 mm.
- [ ] Open `~/firstcut.nc`; the preview shows the square on the board. **Router on.** Play.
- [ ] Pause once mid-side and Resume: the bit lifts, returns, lowers to 1 mm at the plunge feed and carries on.
- [ ] The cut is a closed 50 mm square, 1 mm deep, with no gouge at the pause point, no double cut and no missing
      segment. Measure it: ______ mm by ______ mm.
- [ ] Router off. Note the result in the journal.

## 9. Back to Python

- [ ] `wc-switch python`, open Python's UI: it connects, the files and settings are as Rust left them, and a Play of
      `~/aircut.nc` (air cut setup again) starts normally.

Report: the boxes that failed, what you saw, `~/.WebControl/webcontrol-rs.log`, and
`journalctl -u webcontrol-rs --since "<the time from section 2>"`. Rust becomes the boot default only after this real
cut plus two weeks of normal use (owner decision).

## Where Rust differs from Python on purpose

- **A `$38` write resets the controller** (serial README, Rust-intended deviation 5), as in M2.
- **Stop Z** (the Z panel's Stop) ends a running or paused job too; Python left the job streaming after its `!` had
  flushed the controller.
- **A lost link** pauses the job at the first line the controller had not acknowledged; Python paused at the next line
  it had not sent, skipping what the controller lost.
- **A controller reboot while streaming** stops the job with an alert (Python kept streaming into the rebooted board);
  a reboot while the job is paused, as after a reconnect, keeps it paused.
- **Stop forgets the units a job paused in**; Python kept them, so a later job's Resume could switch units back.
- **Queued commands** (jogs, Send G-code, macros) do not resend the gcode position after each line; streamed lines do.
- **The job lock** (owner decision 2, `contract/README.md` "Job lock"): while a job is running or paused, or a tool
  change waits, settings saves (including the COM port), quick configure, the unit switches, the imports, Restore and
  Send G-code are refused. Python allowed them, and a COM port change mid-cut closed the port without `!`.
- **Never lower before travel** (owner decision, 2026-11-15): every resume and restart, Home, Return to Center and
  triangular Accept rise to the higher of the safe height and the bit's own height before any XY travel. Python always
  went to the safe height, lowering a bit that was higher (`real_katie_neck_back_pause_jog_resume`: 10 mm up, lowered
  to 5 mm, then travelled).
- **51.29x spindle speed**: after the M6 upgrade, a resume that restores `M3`/`M4` follows it with the file's last
  `S` on its own line, since 51.29x leaves the PWM at 0 until an `S` line. On 51.27 nothing changes.
- **A `G38.2` in the file** is tracked like Touch Z: streaming waits behind it, moves are refused, a miss holds the job
  at the probe line, and a probe that ends without `z axis zeroed` alerts. Python streamed on regardless.
