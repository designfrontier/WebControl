# webcontrol-rs

The Rust backend (`spec/RUST-PLAN.md`). It serves the same `frontend/dist` as Python, speaks `contract/` over plain
WebSockets, and shares `~/.WebControl` with the Python install.

| Crate | What |
| --- | --- |
| `wc-protocol` | The browser wire: envelope, WebSocket frame, messages, events, `GET /api/state` |
| `wc-config` | `~/.WebControl` layout and `webcontrol.json`, read and written like `config/config.py` (byte-identical format, `.bak`, a one-time `.pre-rs` snapshot), `setValue`'s rules, `computeSettings`, and the gcode and board libraries |
| `wc-gcode` | Gcode load, sanitising, home shift, T rule, restart state, the 3D preview payload and boards, as Python does |
| `wc-kinematics` | The holey firmware's triangular and quadrilateral inverse/forward, float for float with Python |
| `wc-calibration` | Holey (scipy's trust-region least squares, ported step for step) and triangular calibration, test patterns, Accept's settings and `$` lines |
| `wc-firmware` | The controller's serial protocol, sans IO: line classifier, flow control, boot facts, `$` lines, the boot push and the lines a settings edit sends; the 51.292 upgrade's board and version gates, avrdude lines, flash images (the pinned one embedded, the rollback one cut from a read-back) and settings migration |
| `wc-machine` | The sans-IO core (`Machine::handle(event, now) -> effects`): link, settings, files and the job (Play, Pause, Resume, restart, tool changes, M0, alarms), the run-state gates, GPIO buttons and LEDs; and the tokio shell, where Stop has its own channel, a writer task per port puts `!` and `~` before queued lines, and slow work (the computed position, gcode positions, port open and listing) runs on the blocking pool |
| `wc-sim` | A simulated controller (ring buffer, `ok` timing, `!`/`~`, M0, tool change and `$16=0` manual-Z pauses, `$$`, boot banners, status reports, alarms, probes that touch, miss or have no Z axis, B04, B10, a flash to another EEPROM layout) |
| `wc-server` | The `webcontrol-rs` binary: axum routes (incl. the files, boards and archive routes), `/ws` sessions, the serial port, the Pi's GPIO header (rppal, Linux only), embedded UI and images |
| `wc-testkit` | Test support: the `contract/` schema harness and the `contract/serial/` transcript runner |

## Commands (in `rust/`)

- `cargo test --workspace` (needs `frontend/dist`: `npm ci && npm run build` in `frontend/`)
- `scripts/coverage.sh`: tests under `cargo llvm-cov`, failing below 95% lines for any crate
- `cargo run -p wc-server -- --home "$(mktemp -d)" --port 5055 [--simulate]`: never point `--home` at the real
  `~/.WebControl` while testing; `--simulate` talks to `wc-sim` instead of the serial port
- `cargo test -p wc-machine --test transcripts`: replays every transcript in `contract/serial/` (with
  `transcripts-rust/` in place of the Python ones it overrides)
- `cargo test -p wc-machine --test latency -- --nocapture`: a Stop sent during each piece of slow work (a
  forward-kinematics solve, a gcode goto, a draining or stalled line, a port listing, a port open) must reach the
  port within 5 ms; prints each latency
- `cargo test -p wc-machine --test gates`: every contract action in every run state, each state reached through
  `wc-sim`
- `WC_REGENERATE=1 cargo test -p wc-config --test python_roundtrip`: rewrites
  `tests/fixtures/rust_settings/rust-written.json`, which `tests/test_rust_settings_roundtrip.py` loads in Python
- `cargo test -p wc-firmware --test compute`: the derived settings and their `$` lines against
  `tests/fixtures/compute/cases.json`, which `.venv/bin/python tests/fixtures/compute/generate.py` rewrites from
  Python's `computeSettings`
- `cargo run -p wc-calibration --example calibrate -- <home> holey M1 … M12`: a calibration against a copy of
  `webcontrol.json`, printing the result and the `$` lines Accept would send
- `cargo run --release -p wc-gcode --example gcode_timing -- <file.nc> [home_x home_y]`: times each step of opening
  a gcode file, for the M3 time budget
- `bash deploy/test.sh`: `wc-switch`, `deploy.sh` and `install-pi.sh` against stubs
- `bash deploy/update-test.sh`: `webcontrol-update` against `scripts/release.sh`'s tarballs, a `file://` GitHub API
  and `/api/state`, and stub `sudo`, `systemctl` and binaries (update, backup, checksum, busy, rollback, `--status`)
- `WC_FIRMWARE_HEX=<hex> bash deploy/update-docker-test.sh 0.9.0=<binary> 0.9.1=<binary> 0.9.2=<binary>`:
  `webcontrol-update` for real in arm64 Debian trixie with systemd and a password for `sudo`, against three aarch64
  builds (each made with that `WC_VERSION`) packaged by `scripts/release.sh`
- `bash scripts/release-test.sh`: `scripts/release.sh`'s packaging (contents, checksums, version, firmware pin)
  against stubs
- `bash firmware/docker-build.sh <dir>`: builds the holey 51.292 hex from the Orob-Maslow/Firmware v51.292 tag in
  arm64 Docker and fails unless it matches `firmware/holey-51.292.sha256`; `WC_FIRMWARE_HEX=<dir>/holey-51.292.hex`
  then embeds it in any build (CI's cross-build and `deploy.sh build` do). Without it the upgrade wizard refuses
- `cargo test -p wc-machine --test upgrade`: the upgrade rehearsal against `wc-sim` with a fake avrdude (success,
  sync failure and retry, wrong banner and rollback, EEPROM reset and migration)
- `E2E_BACKEND=rust npm run test:e2e` in `frontend/`: every Playwright spec against the binary (the optical
  calibration case is skipped)

## The Pi

`./deploy.sh build|install|push|rollback [host]` (default `pi@maslow.local`) cross-builds with
`cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.36` (zig from `pip install ziglang`) and installs to
`/opt/webcontrol-rs`. On the Pi, `wc-switch rust|python|status` picks the backend that runs and boots; the two units
conflict, so only one holds the port. `install-pi.sh` also adds `/etc/sudoers.d/webcontrol-rs`, which lets the
service run `systemctl poweroff` (the UI's Turn Off Raspberry Pi) and nothing else. The serial log is
`~/.WebControl/webcontrol-rs.log` (rotated at 5 MB). Each milestone's hardware checklist is in
`spec/rust-checklists/`. The firmware upgrade (Actions → Upgrade holey firmware to 51.292…) runs `avrdude`, which
`install-pi.sh` installs, and keeps each backup in `~/.WebControl/firmware-backups/<time>/`; `M6.md` rehearses an
upgrade and a rollback before the real one.

## Releases

`.github/workflows/release.yml` runs on an arm64 runner. It builds the 51.292 hex with `firmware/docker-build.sh`,
then `scripts/release.sh` checks the hex against `firmware/holey-51.292.sha256`, builds `frontend/dist` and the
aarch64 binary with the UI, help pages and firmware embedded, checks that `webcontrol-rs --version` reports the
release version, and writes:

- `webcontrol-rs-<version>-aarch64.tar.gz`: one directory with `webcontrol-rs`, `install-pi.sh`,
  `webcontrol-update`, `wc-switch`, the unit, the drop-in, the sudoers rule, this README and `rust-checklists/`
- `webcontrol-rs-<version>-aarch64`: the bare binary, for `deploy.sh push`
- a `.sha256` beside each, and `SHA256SUMS` with both

The version is the tag without its `v`; otherwise `git describe --tags --match 'v[0-9]*'` (the short SHA until a
`v*` tag exists).

To cut a release:

1. On a green `master`: `git tag -a v0.2.0 -m "webcontrol-rs 0.2.0"`, then `git push origin v0.2.0`.
2. The Release workflow attaches the assets to a **draft** GitHub Release named after the tag. Rerunning it
   replaces a draft's assets and refuses a published release.
3. Install the draft's tarball on the Pi (below), run the checklists in `spec/rust-checklists/` that the release
   touches, then publish the draft.

Actions → Release → Run workflow builds the same assets from any branch as a workflow artifact, without a release.
On the Mac, `WC_FIRMWARE_HEX=<dir>/holey-51.292.hex scripts/release.sh [out-dir]` does what CI does (zig from
`pip install ziglang`; `WC_VERSION` overrides the version), into `target/release-assets` by default.

Installing a release on the Pi, from the directory holding the tarball and its `.sha256`:

```sh
sha256sum -c webcontrol-rs-0.2.0-aarch64.tar.gz.sha256
tar xzf webcontrol-rs-0.2.0-aarch64.tar.gz
bash webcontrol-rs-0.2.0-aarch64/install-pi.sh webcontrol-rs-0.2.0-aarch64
```

To update from the Mac with a release's binary instead of a local build:
`WC_SKIP_BUILD=1 WC_BINARY=webcontrol-rs-0.2.0-aarch64 ./deploy.sh push`.

## Updating on the Pi

`install-pi.sh` puts `webcontrol-update` in `/usr/local/bin`. On the Pi, as `pi`, with the machine idle:

```sh
webcontrol-update            # the latest published release, if it is newer
webcontrol-update --status   # installed version, the backup, the latest published release
webcontrol-update --rollback # put the backup back
```

It only sees **published** releases: GitHub hides drafts from the API, so publish the draft (Releases, above) before
updating. `--version v0.2.0` installs that tag instead, `--force` reinstalls the installed version, and
`--repo owner/name` (or `WEBCONTROL_REPO`) reads another fork's releases (default `designfrontier/WebControl`).
`GITHUB_TOKEN`, if set, is sent to the API to avoid its rate limit.

An update:

1. Refuses unless the Pi is aarch64 with `curl`, `sha256sum` and `tar`, and `/api/state` shows no job running or
   paused, no tool change or Z adjustment waiting, no probe and no controller firmware upgrade. If the service is not
   answering it carries on and says so.
2. Asks for the `sudo` password once, downloads the tarball and `SHA256SUMS` to a temporary directory, checks the
   checksum and the tarball's contents, and stops with nothing changed if either is wrong.
3. Backs up the current install to `/opt/webcontrol-rs/backup`, deleting the previous backup first (it says so):
   the binary, `webcontrol-rs.service`, the Python unit's drop-in, `wc-switch`, `webcontrol-update`,
   `/etc/sudoers.d/webcontrol-rs`, `~/.WebControl/webcontrol.json` (gcode and boards are not touched by updates) and
   a `MANIFEST` with the version, the date and the files. There is one backup, of the install before the last update.
4. Runs the release's own `install-pi.sh`, restarts `webcontrol-rs` and waits up to 60 s for the unit to be active,
   `/api/state` to answer with `backend` `rust` and the new `version`, and, if a controller was connected before,
   for it to reconnect. With no controller connected it only warns.
5. If that fails, it stops the service, restores every file in the backup, restarts and checks again, and prints
   `ROLLED BACK` or `ROLLBACK FAILED`.

`--rollback` does step 5 by hand, with the same idle check, and keeps the backup, so the next update replaces it.
If the Python backend is the one running (`wc-switch`), the update installs but does not start webcontrol-rs.

The backup only covers the last update. The deeper fallback is still the SSD or the old card (spec/INSTALL-PI.md):
power off, boot from it, and look at the card later.
