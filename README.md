# WebControl

A browser-based controller for the [Maslow CNC](https://www.maslowcnc.com/) router. A Rust backend (`webcontrol-rs`)
runs on a Raspberry Pi beside the machine, talks to the Maslow's Arduino over USB, and serves a React UI to any
browser on the network.

It grew out of [WebControl](https://github.com/WebControlCNC/WebControl), the Python/Flask controller by madgrizzle,
John Boiles, Tinker (gb0101010101) and [the other contributors](https://github.com/WebControlCNC/WebControl/graphs/contributors),
which itself was adapted from the Kivy-based [Ground Control](https://github.com/maslowcnc/groundcontrol). The UI keeps
WebControl's menus and screens, and the backend sends the controller the same bytes WebControl v0.94 did, except where
a change is written down in `contract/serial/README.md`. The Python backend is gone from this repository; it stays in
git history.

## Features

- **Control screen and menus:** jog, Z, home, Play, Pause, Stop, macros and the readouts, with WebControl's G-Code,
  Actions, Settings, Boards and Help menus. One responsive layout for desktop and phone, light and dark.
- **3D toolpath preview** of the loaded G-code and the board, with the sled's position and define-home from the canvas.
- **Tool changes, M0 and pause/resume:** a job stops at a tool change or `M0`, then Resume returns to the file's
  position at a safe height before it plunges. Play mid-file restores the file's modal state.
- **Holey calibration:** cut the pattern, enter the twelve measurements, Calculate and Accept. The fit is a port of
  scipy's least-squares solver and gives the same numbers as the original. Triangular calibration is there too.
- **One-click firmware upgrade to holey 51.292:** Actions → Upgrade holey firmware to 51.292… checks the board and the
  current firmware, backs up the controller's flash and settings, flashes the pinned image with `avrdude`, migrates the
  settings, and rolls back to the backup if the new firmware does not answer.
- **The job lock:** while a job is running or paused, the settings, imports, unit switches and custom G-code are
  refused, so a cut never meets a changed machine or a stray line.
- In-app help, logs, backup and restore, diagnostics, GPIO buttons and LEDs on the Pi's header, and power-off.

## Supported hardware and firmware

- **Raspberry Pi 3B or newer**, running **Raspberry Pi OS Lite (64-bit)**. Trixie is recommended; bookworm works.
  The binary needs a 64-bit ARM system with glibc 2.36 or newer, so 32-bit images and Buster do not work.
- **Maslow controller with holey firmware 51.27**, the firmware WebControl v0.94 shipped. From there the upgrade wizard
  moves it to **holey 51.292**. Install this WebControl before flashing 51.292, and flash it from WebControl: the
  flash resets the controller's EEPROM, and WebControl then writes your settings back instead of adopting 51.292's
  defaults (`spec/firmware.md`, A2). The wizard also keeps the backup a rollback to 51.27 needs.
- The upgrade wizard handles holey firmware only, not stock or custom images. The camera and optical calibration are
  not supported.

## Install on a Pi

[spec/INSTALL-PI.md](spec/INSTALL-PI.md) is the step-by-step guide: a microSD card or SSD beside an existing install,
copying an existing `~/.WebControl`, checking the result, and rolling back by swapping the boot disk. In short:

1. Flash **Raspberry Pi OS Lite (64-bit)** with [Raspberry Pi Imager](https://www.raspberrypi.com/software/) 2.0 or
   newer. In its customisation, set the hostname (`maslow`), the user (`pi`), SSH with your public key, and Wi-Fi.
   Older Imager versions cannot set these on a trixie image.
2. Boot the Pi and copy any existing `~/.WebControl` to it (settings, G-code, boards). Without one, WebControl starts
   from the defaults.
3. Install WebControl, either way:
   - **From a release.** Download `webcontrol-rs-<version>-aarch64.tar.gz` and its `.sha256` from
     [Releases](https://github.com/designfrontier/WebControl/releases) (or from a Release workflow run's artifact in
     Actions), copy them to the Pi, and on the Pi:

     ```sh
     sha256sum -c webcontrol-rs-<version>-aarch64.tar.gz.sha256
     tar xzf webcontrol-rs-<version>-aarch64.tar.gz
     bash webcontrol-rs-<version>-aarch64/install-pi.sh webcontrol-rs-<version>-aarch64
     ```

   - **From a Mac with this repository**, which builds and copies everything over SSH (prerequisites under
     Development):

     ```sh
     rust/deploy.sh install pi@maslow.local
     ```

   `install-pi.sh` puts the binary in `/opt/webcontrol-rs`, installs `avrdude` and the `webcontrol-rs` systemd service,
   and starts it.
4. Open `http://maslow.local:5000/`.

The service runs as the installing user and keeps its settings in `~/.WebControl`, the same layout v0.94 used. Its
serial log is `~/.WebControl/webcontrol-rs.log`; the service log is `journalctl -u webcontrol-rs`.

## Updating

**Cutting a release.** `.github/workflows/release.yml` builds the 51.292 firmware from its tag, checks it against
`rust/firmware/holey-51.292.sha256`, and packages the aarch64 binary (UI, help pages and firmware embedded) with its
install files.

- Actions → Release → Run workflow builds the assets from any branch as a workflow artifact, without a release.
- A `v*` tag on a green `master` attaches them to a **draft** GitHub Release named after the tag:

  ```sh
  git tag -a v0.2.0 -m "webcontrol-rs 0.2.0"
  git push origin v0.2.0
  ```

  Install the draft on the Pi, run the checklists in `spec/rust-checklists/` that the release touches, then publish
  it. `rust/README.md` (Releases) has the details.

**Pushing an update to the Pi.** With the machine idle:

- A release: on the Pi, run the same three install commands with the new tarball, then
  `sudo systemctl restart webcontrol-rs`. `install-pi.sh` never restarts a running service, so a job carries on.
- From the Mac: `rust/deploy.sh push` builds, copies and restarts it, and refuses while a job is running. To push a
  release's binary instead of a local build: `WC_SKIP_BUILD=1 WC_BINARY=webcontrol-rs-<version>-aarch64 rust/deploy.sh push`.
- `rust/deploy.sh rollback` puts the previous binary back.

## Development

### Prerequisites

- Node 24 or newer (the current LTS) and npm.
- Rust through [rustup](https://rustup.rs/); `rust/rust-toolchain.toml` picks the toolchain, its components and the
  aarch64 target.
- For the coverage gate: `cargo install cargo-llvm-cov` and `jq`.
- For the Pi build: `cargo install --locked cargo-zigbuild` and zig (`pip install ziglang`, in a virtualenv if you
  like, with its `bin` on `PATH`).
- For the 51.292 firmware image: Docker (`bash rust/firmware/docker-build.sh <dir>`). Without it, builds leave the
  upgrade wizard disabled.

### Frontend (`frontend/`)

```sh
npm ci
npm run dev           # Vite at http://localhost:5173/, proxying /api, /ws and /static to WEBCONTROL_BACKEND
npm run lint          # eslint
npm run typecheck     # tsc -b
npm test              # Vitest, failing below 95% coverage
npm run build         # frontend/dist, which the Rust binary embeds
npm run format        # prettier (npm run format:check in CI)
npm run contract      # regenerates contract/ after a change in src/protocol/
npx playwright install chromium   # once
npm run test:e2e      # every Playwright spec against webcontrol-rs, built and started on a throwaway home
```

`WEBCONTROL_BACKEND` defaults to `http://localhost:5000`, which macOS's AirPlay Receiver often holds. A simulated
machine for `npm run dev`: `cargo run -p wc-server -- --home "$(mktemp -d)" --port 5055 --simulate` in `rust/`, then
`WEBCONTROL_BACKEND=http://localhost:5055 npm run dev`.

### Backend (`rust/`)

Build `frontend/dist` first: the server embeds it.

```sh
cargo test --workspace
scripts/coverage.sh                    # the tests under cargo llvm-cov; fails if any crate is below 95% of lines
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.36 -p wc-server   # the Pi binary
cargo run -p wc-server -- --home "$(mktemp -d)" --port 5055 --simulate
bash deploy/test.sh                    # wc-switch, deploy.sh and install-pi.sh against stubs
bash scripts/release-test.sh           # the release packaging against stubs
```

Never point `--home` at your real `~/.WebControl` while testing. `--simulate` talks to a simulated holey controller
instead of the serial port. `rust/README.md` lists the crates and the narrower test targets.

### The two contracts

- **`contract/`: the UI and the backend.** JSON Schema for every socket message, action, event and HTTP endpoint,
  generated from the zod schemas in `frontend/src/protocol/`. To change it, edit the schema there, run
  `npm run contract`, bump `contract/VERSION` on a breaking change (its README lists what counts), and implement the
  change in `wc-protocol`. The frontend tests fail when `contract/` is stale or the UI calls something it does not
  list; `wc-testkit`'s harness validates what the Rust server sends and accepts. See `contract/README.md`.
- **`contract/serial/`: the backend and the controller.** JSON transcripts of the exact bytes the backend writes and
  how it reacts to the controller's replies, replayed against the Rust machine by
  `cargo test -p wc-machine --test transcripts`. They are hand-maintained specs. Change one only with the behaviour it
  pins, check it against `spec/firmware.md`, and follow `contract/serial/README.md`, "Adding or changing a transcript".

### Fixtures

`fixtures/` holds frozen golden outputs recorded from the original Python backend: calibration and kinematics, gcode
loading, restart sequences and boards, derived settings, settings files and the help pages. The Rust tests compare
against them. Edit them by hand only for a deliberate behaviour change; each directory's README explains the format.

### CI

`.github/workflows/ci.yml` runs on every pull request and on `master`: the frontend job (format, lint, typecheck,
tests, build); the 51.292 firmware build checked against its pin; and the Rust job (fmt, clippy, tests, the coverage
gate, the deploy and release script tests, the aarch64 cross-build with the firmware embedded, and Playwright against
the binary). `release.yml` is described under Updating.

## Docs map

| Path | What |
| --- | --- |
| [spec/RUST-PLAN.md](spec/RUST-PLAN.md) | How the Rust backend was planned and built, and the owner decisions behind it |
| [spec/firmware.md](spec/firmware.md) | The Maslow firmware's behaviour (51.27, 51.292), what the backend must do about it, the upgrade flow |
| [spec/INSTALL-PI.md](spec/INSTALL-PI.md) | Installing on a Pi, step by step, with a way back |
| [spec/rust-checklists/](spec/rust-checklists/) | Hardware checklists to run on the real machine |
| [contract/](contract/README.md) | The UI ↔ backend contract |
| [contract/serial/](contract/serial/README.md) | The backend ↔ controller contract, and the owner's real jobs |
| [rust/README.md](rust/README.md) | The crates, test targets, Pi deploy and releases |
| [docs/](docs/index.md) | The in-app help pages, embedded in the binary |
| [CLAUDE.md](CLAUDE.md) | Project map and conventions for contributors and coding agents |

## License

GPLv3. See [LICENSE](LICENSE).
