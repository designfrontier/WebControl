# Rust backend: implementation plan

**Status:** the Rust backend has replaced the Python one, which was removed from the repository after `220aa0e`;
the React UI moved from `/next` to `/` (old `/next` addresses redirect). This plan is kept as the record of how the
port was built and why. The Python files it names (`main.py`, `Actions/`, `tests/`, `tools/`, ...) are in git history
at `220aa0e`; `tests/fixtures/` is now `fixtures/`, and `contract/serial/transcripts-rust/` was folded into
`contract/serial/transcripts/`.

The plan for replacing WebControl's Python backend (`main.py`, Flask + gevent) with a Rust binary on the shop Pi 3B.
The inputs are `contract/` (what the React UI and a backend say to each other), `spec/firmware.md` (§Implications B
are the firmware requirements, C is the upgrade flow) and `fixtures/calibration/`. This plan does not restate
them: where it says "per B7" or "wart 5", read that section.

Owner decisions taken as given: Rust; the target is Raspberry Pi OS Lite 64-bit on a Pi 3B, from a microSD card that
also holds the Python install; the two backends run as two systemd services, one at a time; holey calibration uses
`wc-calibration/src/trf.rs`, a step-for-step port of scipy's trust-region-reflective `least_squares` on `nalgebra`,
and must match the fixtures (decision 5, below); optical calibration comes last and is experimental;
the 51.292 upgrade is one click and follows C.

## Owner decisions (2026-11-14)

These override anything below that disagrees:

- **Classic UI dropped** from the Rust backend; `/` redirected to `/next/` (the UI has since moved to `/`).
- **Firmware for the upgrade:** built in CI from the v51.292 tag, pinned by SHA-256; holey upgrades only.
- **One shared `~/.WebControl`** for both backends, with the safeguards in §7.
- **No self-update;** deploy from the Mac.
- **Power-off kept. GPIO kept:** port `Actions/gpioActions.py` behaviour (pin factory, button actions routed
  through the job state machine with debounce, LEDs) using `rppal`, with a no-op fallback off-Pi.
- **Camera and optical calibration dropped for now:** M7 is out of scope; no `wc-optical` crate.
- **OS:** Raspberry Pi OS Lite 64-bit (trixie) recommended, bookworm supported; target glibc 2.36.
- **Warts cleaned up after parity,** with a `contract/VERSION` bump.
- **Coverage: 95% lines for every crate** (not the 90/80 below), enforced in CI.
- **Rust becomes the boot default** after M4 passes a real cut plus two weeks of normal use.

## Owner decisions (2026-11-15)

These also override anything below; `contract/README.md` ("Rust-intended differences") and
`contract/serial/README.md` record the wire-level ones.

- **Resume after an ALARM or overflow is kept.** Streaming halts and the job rewinds to the oldest unacknowledged
  line; after the status-report resync, Play resumes from there (`alarm_mid_cut_rewinds_and_resumes`). Stop also
  clears it.
- **Rust's safety differences are intended:** a link lost mid-job pauses the job and rewinds it; machine commands
  are refused with "The controller is not connected." when no controller is connected; settings lines already
  queued for the controller survive Stop.
- **Minor:** a 409 keeps the default reason phrase (the message is in the body); a cross-origin WebSocket upgrade
  is refused (403); a board is capped at 2^24 one-inch cells; the 126 line limit counts bytes, not characters;
  calibration Accept refuses non-finite values; the live computed X/Y uses float32, as the firmware does;
  `deploy.sh build` without Docker warns and builds without the 51.292 image rather than failing; stored settings
  keep their JSON types.
- **3. Triangular Accept raises Z** to the Z-axis safe height before its `G0 X0 Y0`, and is refused without a finite
  safe height. A Rust-intended deviation (`contract/serial/README.md`).
- **5. Holey calibration uses `wc-calibration/src/trf.rs`**, the scipy TRF port, not the `levenberg-marquardt` crate.
- **Never lower before travel.** Before the XY travel of every resume and restart (Pause, M0, tool change, Play
  mid-file, Play after an alarm), and of Home, Return to Center and triangular Accept, Z goes to max(safe height, the
  bit's Z); no Z move before that travel lowers the bit (`spec/firmware.md` B10). With the bit's Z unknown it waits
  for a fresh status report and is refused with an alert after 3 s. A Rust-intended deviation.
- **51.29x spindle speed.** A restart that restores `M3`/`M4` on 51.29x follows it with the file's last `S` on its own
  line (B13); 51.27 and stock are unchanged. A Rust-intended deviation.

## 0. Recommendations up front

1. **Serve only the React UI.** Drop the classic UI (Jinja `templates/`, `static/scripts`, `WebPageProcessor/`,
   `requestPage`, the `classicOnlyClientToServer` events) from the Rust backend. `/` redirected to `/next/`. The 14
   classic form routes the React UI still posts to (`/openGCode` … `/backupWebControl`, family `classic` in
   `index.json`) are part of the contract and are implemented with their classic shapes (wart 8). The Python backend
   stays the fallback for anything only the classic UI does.
2. **Hand-written protocol types, checked by schema-validation tests.** `cargo typify` was tried on all 216 schemas:
   215 converted and `messages/cameraMessage.json` panicked it. The output is 38k lines of per-file duplicated
   newtypes (`HomePositionMessageXvalString`) and cannot say which form of a numbers-or-text field to *send*, which
   is the whole difficulty (warts 1, 2, 5). Hand-written serde types with a closed-schema test for every value the
   server produces (§3) are smaller and catch the same drift.
3. **A sans-IO machine core.** All job, link and firmware state lives in one state machine that takes events
   (controller line, UI action, clock tick) and returns effects (bytes to write, messages to push, settings to save).
   The tokio code around it only does IO. This removes the gevent races (§1.3) and makes the serial transcripts (§5)
   deterministic replays instead of timing-dependent tests.
4. **One new crate beyond the proposal: `wc-machine`.** It owns the job state machine, which needs `wc-firmware`,
   `wc-gcode` and `wc-config` together. `wc-firmware` stays a protocol library, so the simulator and the upgrade
   tool can use it without the job model.
5. **Pick the transport from `GET /api/state`.** It gains an optional `backend` field (`"rust"`; Python sends
   `"python"`); absent means socket.io (§4).

## 1. Architecture

### 1.1 Workspace

The workspace goes in `rust/` at the repo root (`rust/Cargo.toml`, crates in `rust/crates/`). The binary is
`webcontrol-rs`.

| Crate | Contents | Replaces (Python) | Depends on |
|---|---|---|---|
| `wc-protocol` | Envelope, WebSocket frame, every message/event/action/HTTP body as serde types; the run-state gates; `CONTRACT_VERSION` read from `contract/VERSION` by `build.rs` | `frontend/src/protocol` (as the wire), `api/client_emit.py` | serde |
| `wc-config` | Reads and writes `~/.WebControl/webcontrol.json` compatibly; the settings table from `defaultwebcontrol.json` (embedded); computed settings (`computeSettings`, run at startup, on every settings write, after both imports and after a restore, and checked against Python by `tests/fixtures/compute/`); the directory layout (`gcode/`, `boards/`, `imports/`, `downloads/`); `safePaths` | `config/config.py` minus firmware sync, `File/safePaths.py`, `File/retention.py` | wc-protocol |
| `wc-gcode` | Load filter (spacing, truncate), comment stripping, sanitiser (B5), home shift (`rust_decimal`), the T rule (`firmwareGcode.py`), `gcodeStateAt` and the restart sequence, zMoves, `findPositionAt`, the 3D line model and its gzip payload, board model, cut grid and board file format | `File/gcodeFile.py`, `Connection/firmwareGcode.py`, restart half of `Actions/actions.py`, `Boards/` | wc-protocol |
| `wc-kinematics` | Firmware-exact triangular and quadrilateral inverse/forward (B14), float32 where the firmware is | `Actions/HoleySimulationKinematics.py` | nalgebra |
| `wc-calibration` | Holey `Calibrate` (scipy TRF port, `trf.rs`), test pattern, accept commands; triangular `calculate`, pattern, accept | `Actions/holeyCalibration.py`, `Actions/triangularCalibration.py` | wc-kinematics, nalgebra |
| `wc-firmware` | Sans-IO link: line classifier (status, `[PE:`, `ok`, `error:`, `ALARM:`, `Buffer overflow!`, `Maslow Paused`, `Tool Change:`, `Message:`, boot banners, `Firmware Version`, `$` reports, B04 glued status, 51.291 glued `ok`); character-count flow control (B3, B4, B6); boot detection and version/fingerprint (B2, B8, `firmwareFacts.py`); per-version `$` key table and the sync planner (B9, B12); the avrdude flash procedure (C) | `Connection/serialPortThread.py` (accounting), `Connection/firmwareFacts.py`, `Background/messageProcessor.py` (classification), `config.syncFirmwareKey` and friends, `Actions/firmware.py` | wc-protocol |
| `wc-machine` | The machine core (§1.2): job state machine, gates, pause/tool-change/alarm model, settings sync on boot, PID tests, the UI projection (position/health/status throttling); plus the tokio shell that owns the port | `Actions/actions.py` (dispatch), `Background/UIProcessor.py`, `DataStructures/data.py` | all of the above, tokio, tokio-serial |
| `wc-sim` | Firmware simulator (§5.4) implementing the same port trait; also behind `--simulate` in the binary for e2e and demos | `tests/test_serial_streaming.py` FakePort | wc-firmware |
| `wc-server` | axum: `/api/*`, the classic form routes, `/ws`, embedded `frontend/dist`, `docs/`, `static/images`; sessions; the binary's `main` | `main.py`, `api/`, `next_ui.py` | wc-machine, rust-embed |
| `wc-optical` (M7) | Square finding with imageproc, capture with v4l, behind a cargo feature | `Actions/opticalCalibration.py`, `Background/webcamVideoStream.py` | imageproc, v4l |

Not ported: `WebPageProcessor/`, `templates/`, `HelpManager/` (a no-op), `ReleaseManager` PyInstaller self-update,
`Background/WebMCPProcessor.py` (the `/WebMCP` Docker watchdog), `Actions/gpioActions.py` (GPIO settings are kept in
the file but unused; open question 5).

### 1.2 Concurrency model

```
             ┌──────────── realtime mpsc (Stop, ~) ─────────────┐
 WS session ─┤                                                  ▼
 HTTP route ─┴─ command mpsc + oneshot reply ─► machine task ─► port writer task (owns the write half)
                                                  ▲   │
            port reader task ── ControllerLine ───┤   ├─► broadcast<ServerPush> ─► WS sessions
            blocking pool ── Computed, Ports … ───┤   ├─► watch<Snapshot>  ─► HTTP GETs, /api/state
            clock (tokio interval 10 ms) ─────────┘   └─► persist mpsc ─► config writer task (atomic save)
```

- **The machine task is the only writer of machine state, and its port writer task the only writer to the port.**
  It runs
  `select! { biased; realtime, controller lines, commands, tick }` and feeds each into
  `Machine::handle(event, now) -> Vec<Effect>`. `Machine` is plain synchronous Rust with no tokio and no clock of its
  own, so every test drives it directly with a fake `now`.
- **Stop has its own path.** `stopRun`/`stopZ` (and Resume's `~`) go on the realtime channel, which the biased select
  takes before anything else. The machine emits `!` at once, ahead of the 50 ms line pacing (`MINTimePerLine`), and
  the port writer task writes it before any line still queued there. A `!` also drops those lines and cuts short a
  line half-written; the firmware flushes its buffer on `!` anyway. The writer never flushes: on a serial port that
  is `tcdrain`, which blocks until a line has left (126 bytes at 57600 baud is about 22 ms). The Stop handler never
  waits on a lock, a reply, the job state or slow work (next bullet). `wc-machine/tests/latency.rs` holds a Stop
  sent during each piece of slow work to 5 ms; before this, each one held Stop for its whole length.
- **The reader task** owns the read half (`tokio::io::split` of the tokio-serial stream), splits on `\n`, handles
  B04's glued status line, and sends `ControllerLine`s. It never touches state.
- **Connection management is part of the machine.** It replaces the 5 s `schedule` poller and the connect handler
  that both called `openConnection()` (two `SerialPortThread`s could race to open the port). `Disconnected →
  Opening → AwaitingBoot → Ready` is one enum, and only the machine task opens the port. A firmware upgrade moves it
  to `HandedOff` (§6), which blocks reconnects until the flash finishes.
- **Blocking work leaves the async runtime.** avrdude runs under `tokio::process`; zip backup/restore, gcode parsing
  and the 3D gzip build run in `spawn_blocking` and report back as machine events. Python ran all of these inside
  request handlers. Slow work the machine itself asks for leaves its task too: the forward-kinematics solve behind
  `computedX`/`computedY` (30–60 ms per report on a Pi 3B) and the read from the top of the file behind a gcode
  position (a goto, an index move, a rewind, a G20/G21) are `Effect::Compute(Work)`, which the shell runs on the
  blocking pool and answers with `Event::Computed`. One of each kind runs at a time and only the newest request
  waits. Opening the port (with its 20 ms reset pulse), listing ports and reading the CPU load also run there.
  A closed port still writes what was queued on it (the `!` before a flash) for up to 250 ms before it is dropped,
  and avrdude waits until no port task has the device open or is opening it.
- **Broadcast vs per-client.** Every push is a `ServerPush { client: Option<Sid>, … }` on one `broadcast` channel.
  Each WS session forwards broadcasts, and only its own per-client pushes, which also carry `client` per contract.
  An HTTP request's answer is its response (contract README).

### 1.3 State and the job state machine

The races came from shared mutable fields with several writers: `uploadFlag` (actions, messageProcessor, serial
thread), `gcodeIndex`, `jobInProgress`, `firmwarePaused`/`firmwareHoldsJobLines`, `currentTool`/`nextTool`,
`pausedUnits`, `connectionStatus`, plus `runGeneration`, which exists only to notice a Stop that raced with
`sendNextLine`. In Rust all of them become fields of `Machine`, written only by `Machine::handle`. Readers get an
immutable `Snapshot` through `watch`.

```rust
enum RunState {
    Idle,
    ToolChangeOutsideJob,                     // contract gate row "waiting for a tool change outside a job"
    Running { index: usize },
    Paused { cause: PauseCause, holds_job_lines: bool, resume: ResumePoint },
    Stopping { resync_after: Instant },       // `!` sent; ignore acks until a status report after the input runs dry
}
enum PauseCause { User, ProgramPause, ToolChange { tool: u32 }, ManualZ, ControllerMessage, Alarm, ConnectionLost }
```

- **Gates in one function.** `fn allowed(state: &RunState, gate: Gate) -> bool` implements the README table,
  including the two firmware pauses that refuse machine commands (`holds_job_lines`, and `ManualZ` outside a tool
  change). A test loads `index.json` and checks every action's `allowedDuring` against it (§3.2).
- **Job lock (owner decision 2, 2026-11-15).** The `noJob` lock (`Machine::gcode_locked`: a job in progress or a
  tool change waiting) is the one source of truth for refusing settings writes, quick configure, the unit switches,
  the imports, the restore and `/sendGCode` with 409 (an alert for `updateSetting`), as `contract/README.md` "Job
  lock" lists. It is a Rust-intended deviation; Python allows them.
- **Wire compatibility.** `statusMessage.uploadFlag` (0/1/2/-1), `pauseButtonSetting`, `jobInProgress` and
  `manualZAxisAdjust` are derived from `RunState` in one projection function. They are never stored.
- **`Stopping` is visible.** In Python a Stop was a flag write followed by hoping the serial thread saw it. Here a
  Stop moves to `Stopping`, which refuses streaming until the resync rule (`_awaitResync`: ignore acks, wait for a
  status report read after `RESYNC_DELAY` with no input) moves it to `Idle`.

## 2. Library choices (validated)

Spike: `rust-spike/` in the session scratchpad. It builds axum (ws + multipart), tokio, tokio-serial, serde_json,
rust_decimal, flate2, levenberg-marquardt 0.14 (since replaced, below) + nalgebra 0.33, rust-embed and tracing, with a jsonschema contract
test. It cross-compiles from the arm64 Mac with
`cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.36` (zig from the `ziglang` pip package, so there is
no Homebrew dependency). The binary ran correctly in `linux/arm64` Docker on `debian:bookworm-slim` (glibc 2.36) and
`debian:trixie-slim` (2.41). A separate `optical-spike` (v4l 0.14 + imageproc 0.25) also cross-compiles and runs.

| Need | Crate | Finding |
|---|---|---|
| HTTP + WebSocket | `axum` 0.8 (`ws`, `multipart`) | Builds. The ws upgrade handler is in the spike |
| Runtime | `tokio` 1 | `rt-multi-thread` with 2 workers (the Pi 3B has 4 cores and 1 GB) |
| Serial | `tokio-serial` 5 with `default-features = false` | **Drop the `libudev` feature.** serialport 4.10 then enumerates ports from `/sys/class/tty`, which needs no libudev sysroot for cross builds. Opening `/dev/ttyACM0` returned ENOENT cleanly in Docker |
| JSON | `serde`, `serde_json` | Settings keep `serde_json::Value` for unknown fields |
| Home shift | `rust_decimal` | `95.8967 + 1e-4 = 95.8968` exact. Use `round_dp_with_strategy(MidpointNearestEven)` (Python's Decimal default) and print `-0` as `0` |
| gzip | `flate2` | The payloads are compared by inflated JSON value, never by compressed bytes |
| Least squares | `nalgebra` 0.33 and `wc-calibration/src/trf.rs` | The spike's `levenberg-marquardt` 0.14 converged but stopped elsewhere than Python. `trf.rs` ports scipy 1.9's `least_squares` (TRF, 2-point Jacobian, `diff_step=0.1`, `ftol=1e-11`) step for step, so it lands on Python's five decimals; `tests/fixtures/trf_path.json` pins its iteration path (risk 4) |
| Embedding | `rust-embed` 8 | `frontend/dist` (1.3 MB), `docs/` (1.6 MB), `static/images` (1.3 MB) |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` | journald gets stdout. The log viewer's `/MaslowCNCLogs` feed is a tracing layer |
| Schema tests | `jsonschema` 0.61 (dev) | Draft 2020-12 and the closed-schema trick from `test_contract.py` work in the spike |
| Regex | `regex` | It has no look-around. The Python patterns that use `(?=.)`/`(?![0-9.])` become small hand-written scanners, checked against Python output (§5.2) |
| Optical (M7) | `v4l` 0.14, `imageproc` 0.25, `image` | Cross-compiles with no system libraries |

**Binary size.** The stripped spike is 1.2 MB (thin LTO, `panic = "abort"`). With the full backend (estimated 2–4 MB
of code) and 4.2 MB of embedded assets, expect 7–9 MB. **glibc:** pin the target to `.2.36`, which is bookworm's.
The spike needs `GLIBC_2.34` at most, so the binary also runs on trixie (2.41) and bullseye-64 (2.31 needs testing;
not a target). Do not build natively on `ubuntu-24.04-arm` CI runners without zigbuild: glibc 2.39 there would
produce a binary bookworm cannot load.

## 3. Contract mapping

### 3.1 Types

`wc-protocol` holds hand-written types, one module per `frontend/src/protocol/` slice (`main`, `viewer`, `settings`,
`files`, `boards`, `machine`, `calibration`, `system`), so ownership matches the frontend.

- `Envelope { command, data, data_format }` is built only by `Envelope::json(&T)` (which double-encodes, wart 1),
  `::int` and `::base64`. No other code builds envelopes.
- Numbers-or-text fields are `NumOrText` enums. Each producer picks the variant Python sends today (§3.3).
- Actions are `enum Action { Move { direction: String, distance: NumOrText }, … }`, decoded from
  `{command, arg, arg1}` with the leniency of wart 13.

### 3.2 Drift tests

1. **`wc-server/tests/contract.rs`** (primary). It loads `contract/index.json` and `contract/schemas/` and checks:
   - `CONTRACT_VERSION == contract/VERSION`.
   - Every `http` entry is routed (a request to it never gets axum's 404/405).
   - Every `clientToServer` event has a handler. `Action::COMMANDS` equals the set of `actions` keys.
   - Each action is refused in exactly the states its gate lists. Each state is reached through machine events (a
     simulated M0, tool change, ALARM), not by setting fields as the Python test does.
   - Every GET, every write, every refusal body (the same 11 refusals) and every pushed message validate against
     the closed schemas. A table of message producers must exercise every `serverToClient` command except the
     optical and camera ones until M7.
   - The gzip payloads inflate to `schemas/payloads/*`.
2. **Playwright e2e against Rust.** `frontend/playwright.config.ts` gains `E2E_BACKEND=rust`, which starts
   `webcontrol-rs --simulate --home <tmp> --port 5055` in place of `tests/e2e_backend.py`. The same specs run
   against both backends in CI, so the UI is the final arbiter.
3. `tests/test_contract.py` stays as it is for Python.

### 3.3 The 13 warts

| # | Wart | Rust | Why |
|---|---|---|---|
| 1 | Double-encoded `json` envelopes | Reproduce | The client decodes it. Fixing it is a VERSION bump with no user benefit |
| 2 | Triple-encoded PID results | **Send `data` as the object**, version `"2"` | The schema (`anyOf` string or object) and the UI already accept it. No bump |
| 3 | HTML in `activateModal` | Never sent | Only `requestPage` produced HTML, and that is dropped with the classic UI |
| 4 | Errors are alerts | Reproduce, with the same texts | The UI shows them. The texts are copied from `actions.py` into a `refusals` table |
| 5 | Numbers as text | `homePositionMessage` always sends numbers. `boardDataUpdate` and `requestedSetting.value` send what Python sends, decided by the setting's `type` in `defaultwebcontrol.json` | The schema allows both and the UI coerces. Settings types stay as they are because Python reads the same file |
| 6 | Sentinels | Reproduce | The UI interprets `"none"`, `-999999`, `-1`, `"12.3%"` and `["None"]`. M1 sends `-999999` for computed X/Y until `wc-kinematics` lands |
| 7 | Board cut data and legacy hull boards | Reproduce reading and writing | Boards are shared with the Python install |
| 8 | Classic form route shapes | Reproduce bodies exactly. The reason phrase is the default `Conflict` | The UI never reads `statusText` (checked with grep) |
| 9 | HTML 404s | Return `{"error": …}` JSON 404 | The UI checks only the status. Verify in the M1 PR |
| 10 | 202 for pid-test | Reproduce | Contract |
| 11 | Stray emits (`my response`, `closeModals` top-level) | Not sent | Nobody listens, and there is no socket.io default namespace |
| 12 | Broadcast answers, board resend after every action | Reproduce through M4, then drop the per-action board resend in its own PR if the e2e suite stays green | Other open tabs rely on broadcasts. The resend wastes the Pi's CPU but is harmless |
| 13 | Action args | Accept `""`, numbers and numeric text everywhere | Accepting more is not breaking |

Nothing above needs a VERSION bump. Any later cleanup of warts 1, 4 or 12 is a separate frontend + backend change:
bump `contract/VERSION`, regenerate, and keep both backends passing (the Python side changes too, or the frontend
accepts both forms during the overlap).

## 4. Transport switch

- **`frontend/src/transport/websocket.ts` becomes real.** `connectWebSocket(namespace)` returns a `Transport`
  built on the existing `encodeFrame`/`decodeFrame`/`webSocketUrl`. It reports `connect` only after the server's
  `{"event":"connect","data":{"sid"}}` frame, sends nothing before that, and reconnects with 1 s doubling to a 5 s
  cap, ±50% jitter, forever. A close it did not ask for raises `disconnect`, and a failed open raises
  `connect_error`. `separate` is meaningless here, since every namespace is its own socket. Tests use a fake
  `WebSocket` class injected through a parameter; the 95% frontend floor applies.
- **Choosing the transport.** `connectTransport` becomes `selectTransport(fetch)`: before opening any connection,
  `GET /api/state`, and use WebSockets if `backend === "rust"`, otherwise socket.io. `/api/state` gains an optional
  `backend: "python" | "rust"` in `frontend/src/protocol/main.ts`, which is additive and needs no VERSION bump.
  Python's `api/state.py` sends `"python"`. Probing `/ws` first was rejected because a failed open cannot tell
  "Python" from "Rust is restarting", and it would add a failed handshake before every socket.io connection.
- **Switching backends under an open page.** If the chosen transport fails to connect 3 times in a row, the client
  probes `/api/state` again and reloads the page when `backend` has changed. The owner can then switch services
  without touching the browser.
- **Same build, both backends.** The frontend is built once. Python serves it from `frontend/dist`, and Rust
  embeds the same `dist` at compile time. The Vite dev proxy gains `/ws` (with `ws: true`).
- This is a frontend-only PR stack in M0 (the Python `backend` field is one line in it).

## 5. Test strategy

### 5.1 Golden calibration fixtures

`wc-calibration/tests/fixtures.rs` and `wc-kinematics/tests/fixtures.rs` read `fixtures/calibration/*.json`
directly, with the README's rules:

- floats within `tolerance`, with `keyTolerance` taking precedence
- exact integers, strings, booleans and nulls
- quadrilateral cases evaluated on one fresh model in file order
- `null` expected means the input is refused

`accept_commands.json` is compared line for line, except that the triangular Accept's `G0 Z<safe height>` is
inserted before `G0 X0 Y0` (decision 3). The fixtures stay Python-generated and are never regenerated from Rust.
`wc-calibration/tests/fixtures/trf_path.json` adds scipy's per-iteration path for the holey fit, and
`wc-kinematics/tests/fixtures/firmware32.json` the firmware's float32 kinematics for the live computed position.

### 5.2 Other golden fixtures (new, Python-generated)

Like the calibration ones, a `tests/fixtures/<area>/regenerate.py` plus a Python test checking that Python still
reproduces them:

- `gcode/`: a dozen `.nc` files (from `tests/` and real CAM output: inch and mm, arcs, G91, comments, T/M6 forms,
  long lines). Per file it records:
  - the loaded lines, with and without truncate
  - `zMoves`
  - `moveLine` for several home shifts
  - `gcodeStateAt` and the restart sequence at sampled indices
  - `findPositionAt`
  - the inflated 3D payload, compared by value within 1e-9
- `boards/`: `BoardManager.processGCode` cut grids, `trimBoard`, and the legacy hull-board load.
- `firmware/`: the classifier's verdict for every message line in `spec/firmware.md`, and `toolChangeLine` over a
  table of T/M6 forms.

### 5.3 Recorded serial transcripts

A language-neutral format, under `tests/transcripts/<name>.json`:

```jsonc
{
  "description": "M6 T2 on one line is sent as T2 M6 and stops streaming until Resume",
  "firmware": "51.27",                       // selects the simulated build's quirks
  "settings": { "Maslow Settings": { "bufferOn": "0" }, "Advanced Settings": { "homeX": "12.5" } },
  "gcode": ["G21", "G0 X1 Y1", "M6 T2", "G1 X2 F500"],   // or "gcodeFile": "fixtures/x.nc"
  "controller": {                            // the scripted controller; default: one "ok\r\n" per line, at once
    "boot": ["PCB v1.4 Detected", "Grbl v1.00", "ready", "ok"],
    "rules": [
      { "onSent": "^T2 M6", "hold": "ok", "reply": ["Tool Change: Please insert tool 2"] },
      { "onSent": "^~$", "release": 1 }
    ]
  },
  "steps": [                                 // UI inputs and clock advances, applied in order once the backend is idle
    { "advanceMs": 6000 },
    { "action": "startRun" },
    { "untilSent": "^T2 M6" },
    { "action": "resumeRun" },
    { "untilIdle": true }
  ],
  "expect": {
    "sent": ["G21 ", "$$", "G90 ", "…", "T2 M6 ", "~", "…"],   // every line written, exactly, in order (no \n)
    "messages": [{ "command": "activateModal", "contains": "insert tool 2" }],   // ordered subsequence
    "final": { "uploadFlag": 0, "jobInProgress": false }
  }
}
```

- **Semantics.** Time is logical. The driver applies a step only when the backend is quiescent: nothing more to
  write, or waiting on the controller. Controller rules fire on each written line. `sent` must match exactly. In
  buffered mode, where Python and Rust may interleave writes and acks differently and still be correct, `sent` is
  still exact, and the simulator also asserts that unacked bytes never exceed 126.
- **Recording from Python.** `tests/transcripts/record.py` interprets the same rules as an `on_poll` hook on the
  `SerialHarness`/`FakePort` in `tests/test_serial_streaming.py` and the fake `Clock`. It drives `processAction`
  for `action` steps and writes `expect` from what happened, after review. A Python test replays every transcript,
  so the corpus cannot drift from Python while Python is the reference.
- **Replaying in Rust.** `wc-machine/tests/transcripts.rs` runs `Machine` directly with a `wc-sim` port and a fake
  clock. The same files also run end to end against the binary over a pty pair (`openpty`, Linux CI only).
- **Initial corpus** (about 40 files). Each names the Python tests it is taken from.

| Area | Transcripts | Source tests |
|---|---|---|
| Connect | boot order (units, `$$`, G90, B05, write-only settings); reconnect drops unacked lines; no boot `ok` (A12) | `test_serial_streaming`, `test_firmware_sync` |
| Settings sync | adopt from `$$`; push `$13` once per boot; `$18`/`$9`/`$28`/`$36`; `EEPROM read fail` triggers a full restore; Z limits `$63`/`$64` on 51.292 only | `test_firmware_sync`, `test_z_limits` |
| Home shift and units | G90 shifted, G91 not; Decimal precision; truncate; G20 file on an mm machine; units restored on resume | `test_move_line`, `test_gcode_units`, `test_serial_streaming` |
| Start mid-file | restart sequence: safe Z, XY, `T<n> M6`, spindle, `G1 Z… F<plunge>`, `G<motion> F<feed>`; never lowers to an unset Z | `test_motion`, `test_tool_change_restart` |
| Pause/resume | user pause (no bytes); resume after the index moved; resume at the same index | `test_motion`, `test_job_in_progress` |
| Tool change | `T2`/`M6` on separate lines; `M6 T2`; `T2 M6`; same-tool M6; tool change outside a job; jog during a tool change | `test_tool_change` |
| M0/M1 | M0 alone and at the end of a line; `~` only on Resume; a jog releases the pause only when no job lines are buffered | `test_program_pause`, `test_controller_pause` |
| Stop | Stop mid-line: `!` first, stale acks ignored, resync on status; index rewind | `test_serial_streaming` |
| Alarms | ALARM and `Buffer overflow!` halt and rewind to the oldest unacknowledged line; Play resumes there after the status-report resync, or Stop clears it; connection loss pauses and rewinds | `test_message_processor`, `test_serial_streaming` |
| Refusals | over-long line (127 refused, 126 sent), home shift pushing a line over; unclosed `(`; realtime characters stripped | `test_serial_streaming` |
| Buffered mode | never overfills; same bytes as unbuffered | `test_serial_streaming` |

### 5.4 Firmware simulator (`wc-sim`)

A Rust model of the Maslow firmware's serial behaviour, not its motion:

- the 127-byte ring and overflow-executes-the-tail
- `ok` on line completion, with configurable move durations
- `!`/`~` realtime handling
- M0/M1 and M6 pauses, using its own T semantics
- `$$` dumps and `$` writes, per build (51.27, 51.291 glued `ok`, 51.292 EEPROM notice)
- `EEPROM read fail` after a simulated flash
- boot banners on open
- status reports every 200 ms with MPos at 2 decimals
- ALARM injection

It backs the transcript rules, the contract test's run states, `--simulate` e2e, and the M6 upgrade rehearsal (a
fake avrdude script that rewrites the simulator's version). It is checked against the firmware sources' behaviour as
written in `spec/firmware.md`, and against the transcripts recorded from Python.

### 5.5 Python tests that carry over

Class (a) from the inventory become Rust tests: as unit tests on the crate that owns the behaviour, or as transcripts
where they drive the serial link. These are about 370 of the 600 test functions, and most of the ~980 collected
cases, since the pause and tool-change tests are heavily parametrised.

- `test_actions_characterization`; api: `test_api_{boards,calibration,files,machine,main,settings,state,viewer}`
- `test_board_cut_map`, `test_calibration_{accept,fixtures}`, `test_config_storage`, `test_contract` (as §3.2)
- `test_controller_pause`, `test_firmware_sync`, `test_gcode_{file_load,units}`, `test_holey_{calibration_model,kinematics,test_pattern}`
- `test_job_in_progress`, `test_message_processor`, `test_motion`, `test_move_line`, `test_program_pause`
- `test_restore`, `test_safe_paths`, `test_serial_streaming`, `test_tool_change`, `test_tool_change_restart`, `test_z_limits`

Partly: the parsing half of `test_ui_processor`, the flow (not the paths) of `test_firmware_upgrade`, the error
paths of `test_calibration_routes`, the 409s of `test_gcode_load_guard`, the PID part of `test_api_system`,
`test_log_retention`, and `test_next_ui` (as SPA-serving tests).

Not carried (Python-only): classic socket and UI tests, `test_gpio`, `test_webcam`, `test_release_*`,
`test_firmware_packaging`, `test_api_registry`, and `test_optical_calibration` until M7.

### 5.6 Coverage

`cargo llvm-cov` in CI, with a floor of 90% lines for `wc-gcode`, `wc-kinematics`, `wc-calibration`, `wc-firmware`,
`wc-config` and `wc-machine`'s core, and 80% for the workspace (the tokio shell and `main` are covered by e2e,
which the tool does not count). The frontend keeps its 95%. Python has no floor today, and this plan adds none.

## 6. Milestones

Every milestone ships as a stack of PRs under 1000 source lines each (tests and fixtures excluded). Each PR is split
by behaviour, with only the next one ready for review. **Each milestone is tested on the owner's microSD card**: run
`wc-switch rust`, follow the milestone's hardware checklist (`spec/rust-checklists/Mx.md`, written with the
milestone), then `wc-switch python`. Python stays the default service until M4 passes a real cut.

| M | Scope (contract items) | Done when | Size | Needs | Parallel with |
|---|---|---|---|---|---|
| **M0** Skeleton | Workspace, CI cross-build (§7), `wc-protocol` envelope/frame, `wc-config` read-only; `/ws` handshake, ping and sessions for `/MaslowCNC` and `/MaslowCNCLogs`; embedded `/next/*`, `/static/{filename}`, `/` → `/next/`; `GET /api/state` (with `backend`); frontend WebSocket transport and selection (§4); systemd unit, `wc-switch`, `deploy.sh` | On the Pi, `wc-switch rust` serves the UI over WebSockets showing "disconnected", `wc-switch python` restores the old service, and the same `dist` works on both | ~1.4k src; 3 PRs (one frontend) | – | – |
| **M1** Link | Serial link per B1–B8: boot, B05 version, fingerprint, `connectionStatus`, `positionMessage`, `errorValueMessage` (sentinel), `healthMessage`, `statusMessage`, `controllerMessage`, `alarm`/`clearAlarm`, `activateModal` alerts, `hostAddress`, `updatePorts`; actions `move`, `moveTo`, `moveZ`, `home`, `returnToCenter`, `moveToDefault`, `defineZ0`, `stopRun`, `stopZ`, `statusRequest`, `reportSettings`, `updatePorts`, `macro1`/`macro2`, `clearLogs`; `settingRequest`, `requestedSetting`, `homePositionMessage`; `GET /api/main`, `/api/viewer`, `/api/settings/{id}`, `/api/system/info`; the log namespace; `wc-sim` basics | Contract test green for these items; Connect and Stop transcripts pass; on the machine, connect, jog in all directions, Z up/down, home, Stop mid-jog, read settings, and the firmware version shows | ~2.5k; 4 PRs | M0 | M3a, M5a |
| **M2** Settings | `PUT /api/settings/{id}`, `updateSetting`; `$$` adopt and compare at printed precision; full push after every boot incl. `$13`, `$18`, `$9`, `$28`, `$36`; `EEPROM read fail` → restore; `setMaxZ`/`setMinZ`/`clearZ`/`getZlimits` on 51.292+ only (B12); `GET/POST /api/machine/quick-configure`, `GET /api/machine/setup`; `setSprockets*`, `rotateSprocket`, `resetChainLengths`, `adjustChain`, `testMotors`, `wipeEEPROM`, `setFakeServo`; PID: `GET /api/system/pid`, `POST /api/system/pid-test`, `updatePIDData` | Settings-sync transcripts pass; a Rust-written `webcontrol.json` loads in Python (CI round-trip test); on the machine, edit a setting, power-cycle the controller, see `$13` and the rest re-pushed and `$$` verified | ~1.8k; 3 PRs | M1 | M3, M5 |
| **M3** Files and boards | `/api/files/gcode*`, `/api/boards/*`, classic `/openGCode`, `/uploadGCode`, `/saveGCode`, `/editGCode`, `/sendGCode`, `/importFile`, `/importFileWCJSON`, `/importRestoreWebControl`, `/openBoard`, `/saveBoard`, `/editBoard`, `/trimBoard`, `/downloadDiagnostics`, `/backupWebControl`; `checkForGCodeUpdate`, `checkForBoardUpdate`, `gcodeUpdateCompressed`, `showFPSpinner`, `boardDataUpdate`, `boardCutDataUpdateCompressed`, `updateDirectories`, `gcodePositionMessage`; actions `clearGCode`, `createDirectory`, `boardProcessGCode`, `boardClearBoard`, `moveGcodeZ`/`Goto`/`Index`, `defineHome`, `resetHomeToCenter`. M3b copies Python's load-started signal (`showFPSpinner` `0` broadcast as a load begins; a failed load resends the current preview) and keeps file work off the machine actor via `spawn_blocking`; loads run one at a time and `startRun` is refused while one builds | The gcode and board golden fixtures pass; the e2e `files-boards` and `viewer` specs pass on Rust; on the Pi, a 100k-line file loads and previews within 2× Python's time, and a backup made by Rust restores in Python | ~2.2k; 4 PRs. **M3a** (pure `wc-gcode`: load, shift, state, payload, boards) can start after M0 | M3a: M0. M3b (routes): M1 | M1, M2, M5 |
| **M4** Jobs | `startRun`, `pauseRun`, `resumeRun`, restart mid-file, tool change (all T/M6 forms, outside a job), M0/M1, `Message:` and manual Z, ALARM and overflow recovery, connection-loss pause, over-long and unclosed-comment refusal, buffered mode, the `noJob` 409s, `touchZ`, gates `idleOrControllerPaused`/`noJob` | All transcripts pass; the contract test's per-state gate check passes with every state reached through `wc-sim`; on the machine, air-cut (Z raised, no bit) a file with M0 and a tool change, pause, resume, Stop and restart mid-file, then **one real cut** | ~2.5k; 4–5 PRs | M1, M3 | M5, M6a |
| **M5** Calibration | `wc-kinematics` (inverse/forward, both modes, clamps) and `errorValueMessage` computed X/Y; holey `Calibrate`, test pattern, accept; triangular calculate, pattern, accept; `/api/calibration/{triangular,holey}` (+ `/accept`), actions `cut*CalibrationPattern`, `accept*CalibrationResults` | All four calibration fixture files pass within tolerance; e2e `calibration` spec on Rust; on the machine, holey calibration from saved measurements gives the same numbers as Python (±1e-3) and the accept writes the same `$` lines | ~1.6k; 3 PRs. **M5a** (pure math against fixtures) can start after M0 | M5a: M0. Routes: M2 (accept writes settings) | M1–M4 |
| **M6** Firmware upgrade | `upgradeHoleyFirmware` per C1–C9: pre-flight (banner, B05, `$$`, B10, board hazards), backup (`$$`, settings, chain lengths, **avrdude read-back of the current flash**), pinned SHA-256 hex embedded in the binary, flash with verify and one retry, post-flash checks, settings migration (`$18`=12.6, `$17`, `$63`/`$64`, never `$60`/`$61` on 51.291), guided B04 self-test, rollback; new additive contract items for progress and the wizard (frontend change in `features/system`); `upgradeStockFirmware`/`upgradeCustomFirmware` refuse with an alert | Rehearsal in CI against `wc-sim` with a fake avrdude, covering success, sync failure + retry, wrong banner → rollback, and EEPROM reset → migration; on the Pi, **upgrade 51.27 → 51.292 and roll back once, then upgrade for real**, with the Python install untouched as fallback | ~1.5k + frontend ~0.6k; 4 PRs. **M6a** (pre-flight, backup, artefact pinning, simulator rehearsal) can start after M2 | M2 (settings push), M1 | M4, M5 |
| **M7** Optical (experimental) | `optical_*`, `*OpticalCalibration*`, `opticalCalibration` event, `/api/calibration/optical*`, `cameraMessage`, `updateCalibrationImage`, `updateOpticalCalibration*`, `toggleCamera`/`queryCamera`, behind the `optical` cargo feature | `test_optical_calibration`'s square-finding images give the same centres (±0.5 px); a live capture on the Pi | ~2k; 3 PRs | M5 | – |

**Total to parity (M0–M6):** about 13.5k source lines in 25–27 PRs.

**Critical path:** M0 → M1 → M3b → M4. M3a and M5a are pure, fixture-driven and good agent work from day one. M2,
M5 routes and M6a can follow M1/M2 while M4 is under way.

**Not in any milestone:**
- `update`/`updatePyInstaller` and `/api/system/releases`: return an empty releases list, and the actions alert
  "not supported" (open question 4).
- `shutdown`/`TurnOffRPI`: M1 implements them as `systemctl poweroff` through a sudoers rule, if wanted
  (question 5).

## 7. Packaging and install

- **Build.** `rust/deploy.sh build`:
  1. `cd frontend && npm ci && npm run build`
  2. `cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.36 -p wc-server`
  - `build.rs` in `wc-server` fails the build if `frontend/dist/index.html` is missing, as `pyinstaller_datas.py`
    does. The binary is `webcontrol-rs`, and `webcontrol-rs --version` prints the git SHA and contract version.
- **Layout on the Pi.**
  - Binary: `/opt/webcontrol-rs/webcontrol-rs`, with the previous one kept as `webcontrol-rs.prev`.
  - Settings: the same `~/.WebControl` as Python.
  - Logs: journald plus `~/.WebControl/webcontrol-rs.log`, which the log viewer reads.
- **systemd** `/etc/systemd/system/webcontrol-rs.service`:

  ```ini
  [Unit]
  Description=WebControl (Rust backend)
  After=network-online.target
  Wants=network-online.target
  Conflicts=webcontrol.service

  [Service]
  User=pi
  SupplementaryGroups=dialout
  ExecStart=/opt/webcontrol-rs/webcontrol-rs --port 5000
  Restart=on-failure
  RestartSec=2

  [Install]
  WantedBy=multi-user.target
  ```

  Add a drop-in `/etc/systemd/system/webcontrol.service.d/conflicts.conf` with `Conflicts=webcontrol-rs.service`.
  systemd then stops one backend whenever the other starts, so two services can never hold `/dev/ttyACM0` at once,
  even when started by hand. Both use port 5000, so bookmarks keep working.
- **Switch script.** `/usr/local/bin/wc-switch rust|python|status`:
  1. Refuse if `GET /api/state` says `jobInProgress` (unless `--force`).
  2. `systemctl disable --now` the other unit, then `enable --now` the target. Enabled is what boots.
  3. Wait up to 20 s for `/api/state` to report the expected `backend` (Python answers without the field until its
     one-line change ships, which also counts as Python).
  4. Print the result.

  `rust/install-pi.sh` installs the binary, the unit, the drop-in and `wc-switch`, and `apt install avrdude` (M6).
  It leaves the Python service enabled and running.
- **Config: shared, not copied.** Both backends run as `pi` and never at the same time, so the Rust backend uses
  `~/.WebControl` in place. That way G-code, boards and calibration results never diverge. Safeguards:
  - On its first start Rust copies `webcontrol.json` to `webcontrol.json.pre-rs` once.
  - It writes with Python's format: sorted keys, 4-space indent, raw UTF-8, temp file + rename, and the
    `webcontrol.json.bak` rotation from `config.saveSettings`.
  - It keeps unknown sections, options and option fields untouched.
  - It never changes a stored value's JSON type.
  - CI round-trip test: load the Rust-written file with `config/config.py` and compare.
- **Updating from the Mac.** `rust/deploy.sh push [host]` (default `pi@192.168.86.155`):
  1. Build, then `scp` the binary to `/opt/webcontrol-rs/webcontrol-rs.new`.
  2. Over ssh, run `sudo mv` current → `.prev`, `.new` → current, then `systemctl try-restart webcontrol-rs`. This
     restarts only if Rust is the active backend, so a push never takes the machine from Python.
  3. Refuse if a job is running.

  `deploy.sh rollback` swaps `.prev` back.
- **CI job** `rust` in `.github/workflows/ci.yml`, on `ubuntu-latest`:
  - Steps: `actions/checkout@v7`; `actions/setup-node@v7` with `lts/*`; build `frontend/dist`; install Rust with
    `rustup` (no third-party action); `pip install ziglang` and `cargo install --locked cargo-zigbuild`; then
    `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo llvm-cov` with the §5.6 floors, the
    zigbuild cross-build, and the Playwright e2e against `--simulate`.
  - Upload `webcontrol-rs-aarch64` with `actions/upload-artifact@v7`, the versions the workflow already uses.
  - Confirm each action's `runs.using` is `node24` when the job is written.
  - Cache `~/.cargo` and `rust/target` only with an action that is itself on node24.
- **Releases** `.github/workflows/release.yml`, on `ubuntu-24.04-arm`: a pushed `v*` tag, or a manual run. It
  builds the 51.292 hex as CI does and runs `rust/scripts/release.sh`, which checks the hex against its pin, builds
  with `WC_VERSION` from the tag (else `git describe`), checks that `--version` reports it and that the hex is
  embedded, and packages `webcontrol-rs-<version>-aarch64.tar.gz` (binary, `install-pi.sh`, `wc-switch`, unit,
  drop-in, sudoers rule, `rust/README.md`, `spec/rust-checklists/`) and the bare binary, each with a SHA-256. A tag
  makes a draft GitHub Release with them; a manual run only uploads them as an artifact. No Python bundle.

## 8. Risks and open questions

### Risks, ranked

1. **Streaming behaviour drifts in ways the tests miss** (a missed pause, a lost `!`, a line sent twice), and a real
   router moves. *Mitigation:* sans-IO core, the transcript corpus recorded from Python, simulator invariants (126
   bytes, nothing after ALARM until the resync and then Play or Stop), an air-cut checklist before any real cut, and Python as a one-command
   fallback.
2. **The firmware upgrade misdrives or bricks the controller** (board-strap hazards C1, the TLE9201 encoder swap,
   a lost USB link mid-flash). *Mitigation:* pre-flight blocks unknown and hazardous banners; avrdude read-back
   backup before writing; verify; rollback image; a CI rehearsal against `wc-sim`; and a manual rollback rehearsal
   on the real controller before the real upgrade. The bootloader is never written (`-D`, application hex only), so
   avrdude can always reflash.
3. **The shared `webcontrol.json` is corrupted or reshaped** so that Python misbehaves after switching back.
   *Mitigation:* §7 format rules, preserving unknown fields, `.pre-rs` snapshot, the CI round-trip test.
4. **The holey fit lands outside the fixture tolerances.** scipy uses TRF with a relative 2-point step of 0.1; LM
   with an analytic or finite-difference Jacobian stops elsewhere. *Resolved (decision 5):* `trf.rs` is scipy's
   algorithm, matching every fixture case to Python's five decimals, and
   `wc-calibration/tests/fixtures/trf_path.json` (recorded from scipy by `regenerate_trf_path.py` beside it) holds
   each iteration's nfev, cost, reduction, step and optimality, so a change to the solver's path fails CI.
5. **Port open and reset semantics.** Python toggles parity and reopens to reset the board, then waits for boot.
   tokio-serial asserts DTR on open (HUPCL), which should reset the Mega, but this is unverified. A12 says never
   gate on the boot `ok`. *Mitigation:* M1 explicitly pulses DTR and has a timeout path that sends B05 anyway;
   checklist item on the real board.
6. **Python regex and float semantics in the gcode path** (look-aheads, `float()` leniency, `repr` in the shift,
   truncate regex). *Mitigation:* the §5.2 differential fixtures over real files.
7. **Agents write most of the code without hardware.** *Mitigation:* simulator fidelity checked against Python
   transcripts; every milestone ends with the owner's hardware checklist.
8. **Pi 3B load.** gzip of large 3D payloads and the 25 s pings are cheap in Rust, but 300k-line files allocate.
   *Mitigation:* stream-parse, measure in M3 against Python (2× budget), and keep the 300k render cap.

### Open questions for the owner, ranked

1. **Drop the classic UI from the Rust backend?** Recommended: yes. `/` redirected to `/next/`, and the classic UI
   remained available by switching to Python until Python was removed.
2. **Firmware artefact provenance.** Should CI build v51.292 from the Orob-Maslow tag with `buildFirmware.sh` and
   pin the SHA-256, or should the reviewed shipped hex be committed? Also: support only holey 51.292 (stock and
   custom upgrade buttons alert "use the Python backend")? Recommended: build from the tag, holey only.
3. **Share `~/.WebControl` or copy it?** Recommended: share, with the §7 safeguards.
4. **Self-update.** Is "Check for updates" (`update`, `/api/system/releases`) wanted at all? Recommended: drop it;
   `deploy.sh push` is the update path.
5. **Webcam, GPIO, power-off.** Are any of these used outside optical calibration? Recommended: power-off yes (one
   sudoers line), GPIO no, webcam only with M7.
6. **New SD card OS.** Bookworm or trixie Lite 64-bit? Both run the binary (glibc floor 2.36). The Python install on
   the same card needs uv's Python 3.9 on either.
7. **Wart cleanup.** Fix warts 1, 4 and 12 with a VERSION bump after parity, or leave them? Recommended: after M4,
   only if it removes code on both sides.
8. **Coverage floors** of 90% for core crates and 80% for the workspace: acceptable?
9. **When does Rust become the default** (enabled at boot)? Proposed: after M4's real cut plus two weeks of normal
   use; Python stays installed indefinitely.
