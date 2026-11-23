# Hardware checklists

These checklists validate a WebControl release (`webcontrol-rs` and the React UI) on a real Maslow: a Raspberry Pi
on Raspberry Pi OS Lite 64-bit (`pi@maslow.local`) and a controller on holey 51.27 or 51.292. CI already covers
everything that runs without the machine: the Rust and frontend tests, the transcripts in `contract/serial/` and
Playwright against the simulator. These cover the rest. Start from a green `master` and a release tarball.

They are grouped by feature. Until 0.3.0 there was one file per milestone of the Rust port (`M0.md` to `M7.md`, in
git history up to `dc9bdca`), written while the Python backend still ran beside Rust and checked against it. The
Python backend is gone, so each comparison with it is now a check against the expected serial lines, the expected
numbers, or the transcripts CI replays.

## Index

| Feature | Checklist, section | Moves the machine |
| --- | --- | --- |
| Install from a release tarball, `webcontrol-rs --version` | [`install-update.md`](install-update.md) 1 | No |
| Restarts, reboots, reconnects, Turn off Raspberry Pi | [`install-update.md`](install-update.md) 2 | No |
| `webcontrol-update`: `--status`, update, `--rollback`, refusing while busy | [`install-update.md`](install-update.md) 3-5 | No |
| Update and Roll back from the browser (Help ▸ Update) | [`install-update.md`](install-update.md) 6 | No |
| `rust/deploy.sh push` and `rollback` from the Mac | [`install-update.md`](install-update.md) 7 | No |
| Status bar: server, controller, firmware and version chips | [`ui.md`](ui.md) 1 | No |
| Help pages, About, the log viewer, Clear log files | [`ui.md`](ui.md) 2 | No |
| Display theme, control buttons and macros (Settings ▸ Display, Actions ▸ Macros) | [`ui.md`](ui.md) 3 | No |
| The loaded file's name | [`ui.md`](ui.md) 4 | No |
| Phone and iPad layouts | [`ui.md`](ui.md) 5 | No |
| Connect, the boot handshake, reset on open, unplug and replug | [`connection.md`](connection.md) 1 | No |
| Jog, Z, Home, Return to center, Touch Z | [`connection.md`](connection.md) 2 | Yes |
| Stop mid-move, the alarm hold | [`connection.md`](connection.md) 3 | Yes |
| Editing settings, `$$` after every boot, `$38` | [`settings.md`](settings.md) 1-3 | No |
| Quick configure, setup screens, motors, sprockets, chains, PID | [`settings.md`](settings.md) 4-6 | Yes |
| G-code files, the editor, Send Custom GCode, the load time | [`files-boards.md`](files-boards.md) 1-3 | One 5 mm move |
| Boards | [`files-boards.md`](files-boards.md) 4 | No |
| Backup, restore, diagnostics, imports | [`files-boards.md`](files-boards.md) 5 | No |
| Jobs: Play, Pause, Resume, Stop, M0, tool change, restart mid-file | [`jobs.md`](jobs.md) 1-3 | Yes: an air cut |
| The job lock, refusals, probes in a file, a lost USB link | [`jobs.md`](jobs.md) 4 | Yes |
| Buffered mode, GPIO buttons, LEDs and the status LED | [`jobs.md`](jobs.md) 5, 6 | Yes |
| The first real cut | [`jobs.md`](jobs.md) 7 | Yes: router on |
| Job queue: files back to back, bit checks and the override, stops, Continue, restarts | [`queue.md`](queue.md) | Yes: an air cut |
| The tool library, a file's tools at a tool change | [`tools.md`](tools.md) | Yes: an air cut |
| Parking: Set Park Here, Park, the park after a file or a queue, the refusals | [`parking.md`](parking.md) | Yes: an air cut |
| Lost-line recovery | [`pacing.md`](pacing.md) 1 | Yes: an air cut |
| Line pacing: presets, custom, mid-job changes, run-panel stats, `PACING` log lines | [`pacing.md`](pacing.md) 2-4 | Yes: an air cut |
| Job timer: file estimate, wall, cutting and remaining time, `PACING JOB END` times | [`pacing.md`](pacing.md) 4 | Yes: an air cut |
| Holey and triangular calibration, the test pattern, computed sled position | [`calibration.md`](calibration.md) | Yes: the pattern, in the air |
| Firmware upgrade to holey 51.292, self-test, chain lengths, rollback | [`firmware-upgrade.md`](firmware-upgrade.md) | Yes: the self-test |
| Cameras and optical calibration (experimental, for testers) | [`optical.md`](optical.md) | Yes: the scan |

## Which ones a release needs

Every release, before its draft is published:

- `install-update.md` sections 1 to 4, and sections 5 and 6 once it is published.
- `connection.md`.
- `jobs.md` sections 1 to 3 (the air cut).

Then each checklist whose feature the release changes (`git log v<previous>..v<new>`). A change to streaming, the
job or the serial link (`wc-machine`, `wc-firmware`) also runs `jobs.md` and `pacing.md` in full, real cut included.
A change to the upgrade or a new firmware pin runs `firmware-upgrade.md`.

For a full run, go in the order of the index. Connection comes before anything else that moves; settings before
anything that writes them; files before jobs, since Play needs a file; the air cut before the real cut and before
pacing; calibration after a cut has shown jobs work, since it changes the geometry; the firmware upgrade last.

## Safety, in every checklist

- **Router switched off at its own switch, and the bit removed**, except for the real cuts: `jobs.md` section 7 and
  `pacing.md` section 5.
- The sled has room to move, and you can reach the controller's power.
- Keep a hand near the UI's **Stop** button. If Stop does not stop the sled within a second, cut the controller's
  power, report it, and do not go on.

## Before a run

- [ ] Back up the settings to the Mac, into a new folder, and never edit the original:
      `scp -r pi@maslow.local:.WebControl ~/webcontrol-backup-<version>/`.
- [ ] Note the time, so the logs can be read from there: `date`.

## The way back

- **A box fails:** note what you saw, press Stop if anything moves, and report it (below).
- **A bad release:** on the Pi, `webcontrol-update --rollback` puts back the install the last update replaced
  (`install-update.md` section 3). A release installed from its tarball made no such backup: install the previous
  release's tarball the same way. A binary pushed from the Mac: `rust/deploy.sh rollback`.
- **Bad firmware:** `firmware-upgrade.md` section 7 puts holey 51.27 back.
- **A bad card:** boot the old disk with v0.94 (`spec/INSTALL-PI.md`, step 8), but **only with the controller on
  51.27**. v0.94 never writes `$18`, so on 51.292 its Z speed is whatever the EEPROM holds, and after any EEPROM
  reset that is the factory 126, about ten times too fast. Roll the firmware back first.

## Reading the logs

The log viewer (Help ▸ Logs, `/logs`) and `~/.WebControl/webcontrol-rs.log` on the Pi show each line written as
`Sent: <line>`, followed by what the controller printed. Where a box lists expected lines, read them there. Where it
names a transcript, `contract/serial/transcripts/<name>.json` holds the exact bytes CI replays for that case; the
real machine should send the same lines in the same order (timing and status reports aside).

## Recording and reporting

- Some boxes ask for a number (`______`). Note them in the draft release's notes on GitHub, so the next release has
  numbers to compare against.
- Report the boxes that failed, what you saw, `~/.WebControl/webcontrol-rs.log`,
  `journalctl -u webcontrol-rs --since "<the time you noted>"`, and the zip from Actions ▸ Download diagnostics file.

## Not covered on the machine

- Stock and custom firmware upgrades: the wizard refuses them and points to another tool.
- With no controller connected, WebControl refuses machine commands (jog, Z, sprockets) with "The controller is not
  connected."
