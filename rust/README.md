# webcontrol-rs

The WebControl backend (`spec/RUST-PLAN.md`). It serves `frontend/dist` at `/`, speaks `contract/` over plain
WebSockets, and keeps its settings, gcode and boards in `~/.WebControl`, in the layout WebControl v0.94 used.

It is a port of the Python backend, which is in git history up to `220aa0e`. Doc comments that name a Python file
(`Actions/actions.py`, `config/config.py`, ...) or say "as Python does" point there; `fixtures/` holds that backend's
recorded outputs.

| Crate | What |
| --- | --- |
| `wc-protocol` | The browser wire: envelope, WebSocket frame, messages, events, `GET /api/state` |
| `wc-config` | `~/.WebControl` layout and `webcontrol.json`, read and written like the Python backend's `config/config.py` (byte-identical format, `.bak`, a one-time `.pre-rs` snapshot), `setValue`'s rules, `computeSettings`, and the gcode and board libraries |
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
- `cargo test -p wc-machine --test transcripts`: replays every transcript in `contract/serial/transcripts/`
- `cargo test -p wc-machine --test latency -- --nocapture`: a Stop sent during each piece of slow work (a
  forward-kinematics solve, a gcode goto, a draining or stalled line, a port listing, a port open) must reach the
  port within 5 ms; prints each latency
- `cargo test -p wc-machine --test gates`: every contract action in every run state, each state reached through
  `wc-sim`
- `cargo test -p wc-config --test python_roundtrip`: a v0.94-written `webcontrol.json` is written back byte for byte,
  and typical edits give `fixtures/rust_settings/rust-written.json`; `WC_REGENERATE=1` rewrites that file after a
  deliberate format change
- `cargo test -p wc-firmware --test compute`: the derived settings and their `$` lines against
  `fixtures/compute/cases.json`, recorded from Python's `computeSettings`
- `cargo run -p wc-calibration --example calibrate -- <home> holey M1 … M12`: a calibration against a copy of
  `webcontrol.json`, printing the result and the `$` lines Accept would send
- `cargo run --release -p wc-gcode --example gcode_timing -- <file.nc> [home_x home_y]`: times each step of opening
  a gcode file, for the M3 time budget
- `bash deploy/test.sh`: `wc-switch`, `deploy.sh` and `install-pi.sh` against stubs
- `bash scripts/release-test.sh`: `scripts/release.sh`'s packaging (contents, checksums, version, firmware pin)
  against stubs
- `bash firmware/docker-build.sh <dir>`: builds the holey 51.292 hex from the Orob-Maslow/Firmware v51.292 tag in
  arm64 Docker and fails unless it matches `firmware/holey-51.292.sha256`; `WC_FIRMWARE_HEX=<dir>/holey-51.292.hex`
  then embeds it in any build (CI's cross-build and `deploy.sh build` do). Without it the upgrade wizard refuses
- `cargo test -p wc-machine --test upgrade`: the upgrade rehearsal against `wc-sim` with a fake avrdude (success,
  sync failure and retry, wrong banner and rollback, EEPROM reset and migration)
- `npm run test:e2e` in `frontend/`: every Playwright spec against the binary

## The Pi

`./deploy.sh build|install|push|rollback [host]` (default `pi@maslow.local`) cross-builds with
`cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.36` (zig from `pip install ziglang`) and installs to
`/opt/webcontrol-rs`. `install-pi.sh` enables and starts `webcontrol-rs`, but never restarts a running one (`push`
does, outside a job). On a disk that also has an older WebControl's `webcontrol.service`, the two units conflict, so
only one holds the port, and `wc-switch rust|python|status` picks the one that runs and boots (`python` means
`webcontrol.service`). `install-pi.sh` also adds `/etc/sudoers.d/webcontrol-rs`, which lets the
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

- `webcontrol-rs-<version>-aarch64.tar.gz`: one directory with `webcontrol-rs`, `install-pi.sh`, `wc-switch`, the
  unit, the drop-in, the sudoers rule, this README and `rust-checklists/`
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
