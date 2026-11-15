# webcontrol-rs

The Rust backend (`spec/RUST-PLAN.md`). It serves the same `frontend/dist` as Python, speaks `contract/` over plain
WebSockets, and shares `~/.WebControl` with the Python install.

| Crate | What |
| --- | --- |
| `wc-protocol` | The browser wire: envelope, WebSocket frame, messages, events, `GET /api/state` |
| `wc-config` | `~/.WebControl` layout and `webcontrol.json`, read like `config/config.py` (read-only so far) |
| `wc-machine` | The sans-IO core (`Machine::handle(event, now) -> effects`) and the tokio shell; Stop has its own channel |
| `wc-server` | The `webcontrol-rs` binary: axum routes, `/ws` sessions, embedded UI and images |
| `wc-testkit` | Test support: the `contract/` schema harness and the `contract/serial/` transcript runner |

## Commands (in `rust/`)

- `cargo test --workspace` (needs `frontend/dist`: `npm ci && npm run build` in `frontend/`)
- `scripts/coverage.sh`: tests under `cargo llvm-cov`, failing below 95% lines for any crate
- `cargo run -p wc-server -- --home "$(mktemp -d)" --port 5055`: never point `--home` at the real `~/.WebControl`
  while testing
- `bash deploy/test.sh`: `wc-switch`, `deploy.sh` and `install-pi.sh` against stubs
- `E2E_BACKEND=rust npm run test:e2e` in `frontend/`: the Playwright smoke against the binary

## The Pi

`./deploy.sh build|install|push|rollback [host]` (default `pi@maslow-next.local`) cross-builds with
`cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.36` (zig from `pip install ziglang`) and installs to
`/opt/webcontrol-rs`. On the Pi, `wc-switch rust|python|status` picks the backend that runs and boots; the two units
conflict, so only one holds the port. Each milestone's hardware checklist is in `spec/rust-checklists/`.
