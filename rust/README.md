# webcontrol-rs

The Rust backend (`spec/RUST-PLAN.md`). It serves the same `frontend/dist` as Python, speaks `contract/` over plain
WebSockets, and shares `~/.WebControl` with the Python install.

| Crate | What |
| --- | --- |
| `wc-protocol` | The browser wire: envelope, WebSocket frame, messages, events, `GET /api/state` |
| `wc-config` | `~/.WebControl` layout and `webcontrol.json`, read and written like `config/config.py` (byte-identical format, `.bak`, a one-time `.pre-rs` snapshot), `setValue`'s rules, and the gcode and board libraries |
| `wc-gcode` | Gcode load, sanitising, home shift, T rule, restart state, the 3D preview payload and boards, as Python does |
| `wc-kinematics` | The holey firmware's triangular and quadrilateral inverse/forward, float for float with Python |
| `wc-calibration` | Holey (`levenberg-marquardt`) and triangular calibration, test patterns, Accept's settings and `$` lines |
| `wc-firmware` | The controller's serial protocol, sans IO: line classifier, flow control, boot facts, `$` lines, the boot push and the lines a settings edit sends |
| `wc-machine` | The sans-IO core (`Machine::handle(event, now) -> effects`) and the tokio shell; Stop has its own channel |
| `wc-sim` | A simulated controller (ring buffer, `ok` timing, `!`/`~`, pauses, `$$`, boot banners, status reports) |
| `wc-server` | The `webcontrol-rs` binary: axum routes (incl. the files, boards and archive routes), `/ws` sessions, the serial port, embedded UI and images |
| `wc-testkit` | Test support: the `contract/` schema harness and the `contract/serial/` transcript runner |

## Commands (in `rust/`)

- `cargo test --workspace` (needs `frontend/dist`: `npm ci && npm run build` in `frontend/`)
- `scripts/coverage.sh`: tests under `cargo llvm-cov`, failing below 95% lines for any crate
- `cargo run -p wc-server -- --home "$(mktemp -d)" --port 5055 [--simulate]`: never point `--home` at the real
  `~/.WebControl` while testing; `--simulate` talks to `wc-sim` instead of the serial port
- `cargo test -p wc-machine --test transcripts`: replays `contract/serial/` (with `transcripts-rust/` in place of the
  Python ones it overrides); the `PASSING` list names those the backend must already pass
- `WC_REGENERATE=1 cargo test -p wc-config --test python_roundtrip`: rewrites
  `tests/fixtures/rust_settings/rust-written.json`, which `tests/test_rust_settings_roundtrip.py` loads in Python
- `cargo run -p wc-calibration --example calibrate -- <home> holey M1 … M12`: a calibration against a copy of
  `webcontrol.json`, printing the result and the `$` lines Accept would send
- `cargo run --release -p wc-gcode --example gcode_timing -- <file.nc> [home_x home_y]`: times each step of opening
  a gcode file, for the M3 time budget
- `bash deploy/test.sh`: `wc-switch`, `deploy.sh` and `install-pi.sh` against stubs
- `E2E_BACKEND=rust npm run test:e2e` in `frontend/`: the Playwright specs the binary passes so far (the Rust
  smoke, `files-boards`, `viewer`; `rustSpecs` in `playwright.config.ts`)

## The Pi

`./deploy.sh build|install|push|rollback [host]` (default `pi@maslow-next.local`) cross-builds with
`cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.36` (zig from `pip install ziglang`) and installs to
`/opt/webcontrol-rs`. On the Pi, `wc-switch rust|python|status` picks the backend that runs and boots; the two units
conflict, so only one holds the port. `install-pi.sh` also adds `/etc/sudoers.d/webcontrol-rs`, which lets the
service run `systemctl poweroff` (the UI's Turn Off Raspberry Pi) and nothing else. The serial log is
`~/.WebControl/webcontrol-rs.log` (rotated at 5 MB). Each milestone's hardware checklist is in
`spec/rust-checklists/`.
