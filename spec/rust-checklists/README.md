# Rust backend hardware checklists: the order to run them

These checklists take `webcontrol-rs` from "installed beside Python" to "cutting and upgrading firmware" on the
microSD card (`pi@maslow-next.local`). The SSD with v0.94 is never touched. Run them in the order below. Each one
assumes every earlier one passed. CI already covers everything that can run without the machine (each checklist's
section 1 or "CI" boxes), so start each one with a green `master`.

## The way back, at every step

The Python backend stays installed and working on the card throughout. Only one backend holds port 5000 at a time.

- **On the Pi:** `wc-switch python` stops `webcontrol-rs`, starts the Python `webcontrol` service and makes it the one
  that boots. `wc-switch status` says which one is running.
- **If a box fails:** note what you saw, press Stop if anything moves, run `wc-switch python`, and report the box
  together with `~/.WebControl/webcontrol-rs.log` and `journalctl -u webcontrol-rs --since "<time you started>"`.
- **If the sled does not stop within a second:** cut the controller's power first, then `wc-switch python`.
- **A bad binary:** `rust/deploy.sh rollback` (from the Mac) puts the previous `webcontrol-rs` back. Python is not
  affected.
- **A bad card:** power off, pull the microSD card, power on. The Pi boots v0.94 from the SSD as before
  (`spec/INSTALL-PI.md`, step 8).

Both backends share `~/.WebControl`. Each checklist that writes it starts with a backup to the Mac
(`scp -r pi@maslow-next.local:.WebControl ~/webcontrol-backup-<milestone>/`). Never edit the original.

## Order

| # | Checklist | What it proves | Moves the machine | Way back |
| --- | --- | --- | --- | --- |
| 0 | [`../INSTALL-PI.md`](../INSTALL-PI.md) | The card boots 64-bit Raspberry Pi OS Lite (trixie, or bookworm) with the Python backend working | No | Pull the card |
| 1 | [`M0.md`](M0.md) | `rust/deploy.sh install`, `wc-switch rust`: the UI over plain WebSockets, the help pages, the log viewer, restarts | No (the port is never opened) | `wc-switch python` |
| 2 | [`M1.md`](M1.md) | The serial link: connect, firmware version, jog, Z, home, Stop mid-jog, ports, power-off | Yes: jogs | `wc-switch python` |
| 3 | [`M2.md`](M2.md) | Settings saved in Python's format, the full settings push and `$$` check after every controller boot, setup screens | Yes: motors and sprockets | `wc-switch python`, then check Python reads the shared file |
| 4 | [`M3.md`](M3.md) | Gcode and board files, the time budget, backup, restore, diagnostics and imports (M3a and M3b) | No | `wc-switch python`; Python opens what Rust saved |
| 5 | [`M4.md`](M4.md) | Jobs: an air cut (Play, Pause, Resume, Stop, restart, M0, tool change), refusals, GPIO, then one real cut | Yes: an air cut, then a real cut | `wc-switch python`; Play there once |
| 6 | [`M5.md`](M5.md) | Calibration: holey and triangular Calculate and Accept give Python's numbers, and the holey test pattern is air-cut (M5a and M5b) | Yes: the test pattern, in the air | `wc-switch python`; the calibration page shows what Rust left |
| 7 | [`M6.md`](M6.md) | The holey 51.292 upgrade: rehearsal, self-test, rollback to 51.27, and then the real upgrade | Yes: the self-test turns motors | `wc-switch python` after each part; the rollback in section 6 restores 51.27 |

Why this order:

- M0 never opens the serial port, so it checks the install and `wc-switch` before anything can move.
- M2 comes before any later step that changes settings. It proves that Rust writes `webcontrol.json` in a form Python
  reads back, so `wc-switch python` stays a safe way back.
- M3 comes before M4, because Play needs a loaded file.
- M4's real cut runs on the calibration Python already made. M5 then changes the calibration, so it comes after the
  cut has proved that jobs work.
- M6 comes last. It changes the controller's firmware, and every earlier checklist assumes holey 51.27. Do not start
  the real upgrade (M6 section 7) until the rollback rehearsal (section 6) has passed. After the upgrade, Python also
  runs against 51.292 (M6 section 7), so `wc-switch python` is still the way back.

## Not covered on the machine

- The camera and optical calibration are out of scope for the Rust backend. Use Python for them (`wc-switch python`).
- Stock-firmware and custom-hex upgrades: the Rust wizard tells you to use the Python backend for these.
- With no controller attached, Rust refuses machine commands (jog, Z, sprockets) with "The controller is not
  connected." Python queues them instead.
