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
| `wc-optical` | Experimental optical calibration (`spec/optical-calibration.md`): frames, square detection, the error grid, the synthetic and printable targets, and the cameras (USB over V4L2 on Linux, the Pi camera through `rpicam-vid`); off unless the `opticalCalibrationEnabled` setting is on |
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
  deliberate format change. Loading gives every stored option that the defaults also have their metadata (all but
  `value`: `title`, `desc`, `type`, `options`, `firmwareKey`, ...), so `python-written.json`'s metadata must match
  `defaultwebcontrol.json`; that alone never rewrites the file, but the next save writes it
- `cargo test -p wc-firmware --test compute`: the derived settings and their `$` lines against
  `fixtures/compute/cases.json`, recorded from Python's `computeSettings`
- `cargo run -p wc-calibration --example calibrate -- <home> holey M1 … M12`: a calibration against a copy of
  `webcontrol.json`, printing the result and the `$` lines Accept would send
- `cargo run --release -p wc-gcode --example gcode_timing -- <file.nc> [home_x home_y]`: times each step of opening
  a gcode file, for the load time in `spec/rust-checklists/files-boards.md`
- `bash deploy/test.sh`: `wc-switch`, `deploy.sh` and `install-pi.sh` against stubs
- `bash deploy/update-test.sh`: `webcontrol-update` against `scripts/release.sh`'s tarballs, a `file://` GitHub API
  and `/api/state`, and stub `sudo`, `systemctl` and binaries (update, version order, backup, checksum, busy, rollback,
  `--status`)
- `WC_FIRMWARE_HEX=<hex> bash deploy/update-docker-test.sh 0.9.0=<binary> 0.9.1=<binary> 0.9.2=<binary>`:
  `webcontrol-update` for real in arm64 Debian trixie with systemd and a password for `sudo`, against three aarch64
  builds (each made with that `WC_VERSION`) packaged by `scripts/release.sh`
- `bash scripts/release-test.sh`: `scripts/release.sh`'s packaging (contents, checksums, version, firmware pin)
  against stubs
- `bash firmware/docker-build.sh <dir>`: builds the holey 51.292 hex from the Orob-Maslow/Firmware v51.292 tag in
  arm64 Docker and fails unless it matches `firmware/holey-51.292.sha256`; `WC_FIRMWARE_HEX=<dir>/holey-51.292.hex`
  then embeds it in any build (CI's cross-build and `deploy.sh build` do). Without it the upgrade wizard refuses
- `bash firmware/docker-build.sh <dir> 51.27`: builds `<dir>/holey-51.27.hex`, the holey 51.27 that v0.94 bundled
  (madgrizzle/Firmware @ 950fb23), and fails unless it matches `firmware/holey-51.27.sha256`. It is never embedded;
  it is the last-resort rollback image when no firmware backup is left (`spec/rust-checklists/firmware-upgrade.md`
  section 7)
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
`~/.WebControl/webcontrol-rs.log` (rotated at 5 MB). The hardware checklists for validating a release are in
`spec/rust-checklists/`, grouped by feature. The firmware upgrade (Actions → Upgrade holey firmware to 51.292…) runs
`avrdude`, which `install-pi.sh` installs, and keeps each backup in `~/.WebControl/firmware-backups/<time>/`;
`firmware-upgrade.md` there walks through the upgrade, the self-test and a rollback.

Behind a reverse proxy (nginx) or HTTPS, the proxy must forward the original `Host` (`proxy_set_header Host $host;`)
and the WebSocket `Upgrade`/`Connection` headers: `wc-server` refuses a WebSocket or write whose `Origin` does not
match `Host`, and any from a browser whose `Host` is not a local name (`crates/wc-server/src/guard.rs`,
`contract/README.md` "Cross-site requests"). `spec/INSTALL-PI.md` has an nginx example. Direct
`http://maslow.local:5000` use needs none of this.

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
   touches (its `README.md` says which), then publish the draft.

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

Help ▸ Update in the UI updates and rolls back with the buttons there (below, "From the UI"). The same updater runs
from a terminal: `install-pi.sh` puts `webcontrol-update` in `/usr/local/bin`. On the Pi, as `pi`, with the machine
idle:

```sh
webcontrol-update            # the latest published release, if it is newer
webcontrol-update --status   # installed version, the backup, the latest release, and whether it is an update
webcontrol-update --rollback # put the backup back
```

It only sees **published** releases: GitHub hides drafts from the API, so publish the draft (Releases, above) before
updating. Versions compare as `MAJOR.MINOR.PATCH`, each field as a number (a leading `v` is ignored); a pre-release
such as `0.4.0-rc.1` comes before `0.4.0`, and a `git describe` build such as `0.4.0-3-gabc1234` after it. If the
installed version is newer than the latest release (a build pushed from the Mac, or a release installed with
`--version`), it changes nothing and says so. An installed version it cannot parse is updated whenever it differs,
with a warning. `--version v0.2.0` installs that tag instead, older or not (with a warning when it is older);
`--force` reinstalls the installed version but never downgrades; and
`--repo owner/name` (or `WEBCONTROL_REPO`) reads another fork's releases (default `designfrontier/WebControl`).
`GITHUB_TOKEN`, if set, is sent to the API (on curl's stdin, not its arguments) to avoid its rate limit.

An update:

1. Refuses unless the Pi is aarch64 with `curl`, `sha256sum` and `tar`, and `/api/state` shows no job running or
   paused, no tool change or Z adjustment waiting, no probe and no controller firmware upgrade, and the `busy` of
   `GET /api/system/updates` shows no optical scan. If nothing answers it
   carries on only when `webcontrol-rs` is not running; a running service that does not answer may be mid-job, so it
   refuses until you stop it.
2. Asks for the `sudo` password once (not when it runs as root, from the UI), downloads the tarball and `SHA256SUMS` to a temporary directory, checks the
   checksum and the tarball's contents (no paths outside its directory, no links), and stops with nothing changed if
   either is wrong. The checksum catches a corrupt download; anyone who can publish a release can also run its
   `install-pi.sh` here, so treat write access to the repository as root on the Pi.
3. Backs up the current install to `backup.new`, then swaps it in for the previous backup (it says so), so a backup
   that fails partway leaves the previous one:
   the binary, `webcontrol-rs.service`, `webcontrol-update@.service`, the drop-in for an older WebControl's unit,
   `wc-switch`, `webcontrol-update`,
   `/etc/sudoers.d/webcontrol-rs`, `~/.WebControl/webcontrol.json` (gcode and boards are not touched by updates) and
   a `MANIFEST` with the version, the date and the files. As root, it reads and writes `webcontrol.json` as the
   service user (`runuser`), so a link that user plants in their home cannot point root's copy elsewhere. There is one backup, of the install before the last update.
4. Runs the release's own `install-pi.sh` and checks `/api/state` again: if a job started meanwhile, it puts the old
   files back without restarting anything and stops. Otherwise it restarts `webcontrol-rs` and waits up to 60 s for
   the unit to be active,
   `/api/state` to answer with `backend` `rust` and the new `version`, and, if a controller was connected before,
   for it to reconnect. With no controller connected it only warns.
5. If that fails, it stops the service, restores every file in the backup (a sudoers rule only if `visudo -c` passes
   it), restarts and checks again, and prints `ROLLED BACK`, `ROLLBACK INCOMPLETE` (naming each file it could not put
   back) or `ROLLBACK FAILED`.

`--rollback` does step 5 by hand, with the same idle check, and keeps the backup, so the next update replaces it.

The backup only covers the last update. The deeper fallback is still the SSD or the old card (spec/INSTALL-PI.md):
power off, boot from it, and look at the card later.

### From the UI

The Update screen (Help ▸ Update, `GET /api/system/updates`) shows the installed version and commit, the latest
published release with its notes, whether it is newer, and the backup. The server asks GitHub at most every 10 minutes
(30 s after a failure), with a 5 s timeout, and reads `WEBCONTROL_REPO`, `GITHUB_TOKEN` and `WC_GITHUB_API` from its
own environment (`sudo systemctl edit webcontrol-rs` to set them), not your shell's.

**Update to vX** (only when a newer release is published) and **Roll back to vY** (only when there is a backup) each
ask first, then `POST /api/system/updates/install` or `/rollback`. The server starts
`webcontrol-update@update.service` or `webcontrol-update@rollback.service` with
`sudo --non-interactive /usr/bin/systemctl start --no-block <unit>`, which `/etc/sudoers.d/webcontrol-rs` allows
the service user exactly, with no password, and nothing else beyond `systemctl poweroff`. The unit
(`deploy/webcontrol-update@.service`) runs `webcontrol-update --service update|rollback` as root: the same steps as
above, with no prompts, the latest published release only (no `--version`, `--force` or `--repo`, so never a
downgrade), the repository (checked to be `owner/name`) and token from `webcontrol-rs`'s environment, which only
root can change, and one run at a time. Nothing from the
request reaches the unit or a shell. Being its own unit, it carries on while it restarts `webcontrol-rs`.

The updater writes its progress to `/var/lib/webcontrol-rs/update.json` (root's, mode 644) and to the journal
(`journalctl -u 'webcontrol-update@*'`): the action, the phase (`checking`, `downloading`, `verified`, `backingUp`,
`installing`, `restarting`, `healthCheck`, `rollingBack`, `finished`), the versions, the times, the result
(`updated`, `upToDate`, `rolledBack`, `rollbackFailed`, `failed`) and its log. The page polls the progress every 2 s,
says it is reconnecting while WebControl restarts, and shows the result once the new version answers; reloading the
page mid-update picks the run up again. A run whose unit stopped without finishing reads as `interrupted`.

The server refuses with 409 while an update runs, while the machine is busy (a job running or paused, a hold, a tool
change, a probe, a controller firmware upgrade or an optical scan; the buttons say why and stay off), with nothing
newer to install or no backup to restore, and on an install whose `/etc/systemd/system/webcontrol-update@.service`
is missing: an install older than 0.4.0 updates once from the terminal, which adds the unit and the rule.

Anyone who can open the page can update to, or roll back from, the fork's latest published release, as they can power
the Pi off. That is the intended reach: the release is still checked against `SHA256SUMS` and rolled back if it does
not come up.
