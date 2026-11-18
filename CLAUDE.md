# CLAUDE.md

WebControl controls a Maslow CNC router: a Rust backend (`rust/`) on a Raspberry Pi serves a React UI (`frontend/`)
at `/`. The Python backend this grew from was removed; it is in git history up to `220aa0e`.

## Project structure

### Key directories

- `.github/workflows/` — `ci.yml`: the frontend job (format, lint, typecheck, test, build), the holey 51.292 firmware
  build checked against its pin, and the Rust job (fmt, clippy, tests, 95% per-crate coverage, the deploy and release
  script tests, the aarch64 cross-build, Playwright against the binary); `release.yml` builds the Pi install assets
  (`rust/scripts/release.sh`) and drafts a GitHub Release for a `v*` tag
- `contract/` — frozen frontend ↔ backend spec as JSON Schema, generated from `frontend/src/protocol/` (see its
  `README.md`: layout, versioning, plain-WebSocket wire format, known warts)
- `contract/serial/` — backend ↔ controller contract: hand-maintained JSON transcripts of the exact bytes sent and how
  controller replies are handled, replayed by `cargo test -p wc-machine --test transcripts` (see its `README.md`)
- `docs/` — the in-app help pages (markdown with Jekyll front matter, which the server strips), embedded in the binary
- `docs/Actions/` — help pages for each UI action
- `docs/Actions/Calibration-Setup/` — help for calibration and machine setup actions
- `docs/Actions/Controller/` — help for firmware upgrade and settings sync
- `docs/Actions/Diagnostics-Maintenance/` — help for diagnostics, logs, backups
- `docs/assets/` — images used by the help pages
- `docs/Boards/` — board management help
- `docs/GettingStarted/` — first-steps setup guide
- `fixtures/` — frozen golden outputs of the original Python backend (calibration, kinematics, gcode, boards, derived
  settings, settings files, help pages) that the Rust tests compare against; each area's `README.md` gives the format
- `frontend/` — the React + TypeScript UI (see "Frontend" below)
- `rust/` — the backend `webcontrol-rs` (`spec/RUST-PLAN.md`; crates, commands, Pi deploy and releases in
  `rust/README.md`)
- `rust/crates/` — `wc-protocol`, `wc-config`, `wc-gcode`, `wc-kinematics`, `wc-calibration`, `wc-firmware`,
  `wc-machine`, `wc-optical` (experimental optical calibration), `wc-sim`, `wc-server` (the binary), `wc-testkit`
- `rust/deploy/` — the systemd unit, sudoers rule, `wc-switch` and their tests (`test.sh`)
- `rust/firmware/` — the holey 51.292 build (embedded) and the 51.27 rollback build, each with its SHA-256 pin
- `spec/` — `RUST-PLAN.md`, `firmware.md` (the firmware's behaviour and requirements), `INSTALL-PI.md`, and
  `rust-checklists/` (hardware checklists by feature for validating a release; its `README.md` maps features to
  sections)
- `static/images/` — images the UI shows, embedded in the binary and served at `/static/images/`
- `tools/make_doc_links_relative.sh` — rewrites absolute GitHub links in `docs/` to relative ones

### Key files

- `defaultwebcontrol.json` — every setting with its default and firmware key; embedded by `wc-config`
- `rust/install-pi.sh` — runs on the Pi: installs `/opt/webcontrol-rs`, the unit and `avrdude`, then starts it
- `rust/deploy.sh` — `build|install|push|rollback [host]` from the Mac
- `rust/scripts/coverage.sh`, `rust/scripts/release.sh` (and `release-test.sh`)

## Frontend

### Layout

- `frontend/src/protocol/` — zod schemas for the socket and API contract, one file per feature (`main`, `viewer`,
  `settings`, `files`, `boards`, `machine`, `calibration`, `system`), each exporting `<x>Messages` (server -> client
  `message` commands), `<x>Actions` (processAction command names) and `<x>Events` (client -> server events);
  `core.ts` holds the envelope; `index.ts` only merges them; `actionArgs.ts` types each action's `arg`/`arg1` and
  run-state gate; `http.ts` catalogs every HTTP endpoint the UI calls with its request and response schemas
- `frontend/src/contract/` — `buildContract()` turns the protocol into `contract/`; its tests fail when `contract/`
  is stale or a UI request is missing from `http.ts`. Regenerate with `npm run contract`
- `frontend/src/transport/` — the `Transport` interface the app talks to, and `websocket.ts`, the plain-WebSocket
  transport (`contract/README.md`) that `connectTransport` opens
- `frontend/src/socket/` — `createClient(transport)`: validates messages, drops ones whose `client` field names
  another socket, `sendAction(command, arg?, arg1?)`, `requestSetting`, `updateSetting`, typed `emit`;
  `connect.ts` opens the `/MaslowCNC` and log connections
- `frontend/src/api/` — `getJson`/`sendJson(fetch, path, schema)` against `/api`
- `frontend/src/features/files/api.ts` — `postForm(fetch, path, fields)` posts FormData to the form routes
  (`/openGCode`, `/trimBoard`, ...); each such route also needs an entry in the `vite.config.ts` dev proxy
- `frontend/src/state/` — machine store (reducer + `useSyncExternalStore`) fed by the socket; reloads `GET /api/state`
  on every reconnect
- `frontend/src/app/` — shell: `App` (routes: `/logs`, `/calibration/*`, everything else `ControlLayout`),
  `ControlLayout` (top bar + control screen + route modal), `TopBar`/`Menu` (the classic menus), `modals.ts` (every
  route modal's path, title and body, plus old-address redirects), `Dialog` (the one native `<dialog>` modal;
  `dialogLayer.ts` keeps server dialogs above user ones), `modalHistory.ts` (`useCloseModal`; Back closes a modal),
  `theme.tsx`, `StatusBar`, `WebControlProvider`, hooks (`useClient`, `useFetch`, `useMachine(selector)`,
  `useServerMessage(command, handler)`)
- `frontend/src/features/<x>/` — one folder per slice; `index.ts` re-exports the components `app/` uses. The control
  screen is `features/main`'s `MainPage`, with `features/viewer`'s `Viewer` beside the controls
- `frontend/src/test/` — `renderApp(route, opts)`, `renderWithProviders(ui, opts)`, `createFakeSocket()`
  (`serverConnect`, `serverMessage(command, data)`, `emitted`, `actions()`), `fakeFetch(routes)`, `startupState()`;
  `filesBoards.ts` (`expectPath`, `recorder`, `postedForm`, library and board fixtures)
- `frontend/src/styles/theme.css` — design tokens (custom properties); `app.css` — shell layout
- `frontend/e2e/` — Playwright specs, run against `webcontrol-rs` on a throwaway home

### Slices

A slice's `index.ts` only re-exports its components; logic goes in a sibling module, because coverage skips
`index.ts` files. A new modal screen needs an entry in `app/modals.ts`; `protocol/index.ts` already imports every
slice. A new endpoint or socket event goes into `frontend/src/protocol/` first (and `protocol/http.ts` for an
endpoint), then `npm run contract`, then the Rust side (`wc-protocol`, the route in `wc-server`, the behaviour in
`wc-machine`).

## Commands

- Frontend (in `frontend/`, Node 24+): `npm ci`, `npm run dev` (Vite at http://localhost:5173/, proxying `/api`, `/ws`,
  `/static` and the form routes to `WEBCONTROL_BACKEND`, default http://localhost:5000), `npm run lint`,
  `npm run typecheck`, `npm test` (Vitest + v8 coverage, 95% floor), `npm run build` (to `frontend/dist`, which the
  Rust binary embeds), `npm run format`, `npm run contract` (after any `src/protocol` change)
- E2E: `npm run test:e2e` in `frontend/` builds the app and `webcontrol-rs`, then serves it on port 5055 (override
  with `E2E_BACKEND_PORT`) from a temporary home, plus a `--simulate` one on the next port (`E2E_SIM_PORT`) for the
  optical end-to-end spec. First time: `npx playwright install chromium`
- Rust (in `rust/`, after `npm run build`): `cargo test --workspace`, `scripts/coverage.sh` (95% lines per crate),
  `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.36 -p wc-server` (zig from `pip install ziglang`),
  `bash deploy/test.sh`, `bash scripts/release-test.sh`
- Run locally: `cargo run -p wc-server -- --home "$(mktemp -d)" --port 5055 --simulate`. Never point `--home` at the
  real `~/.WebControl` for testing

## Conventions

- Style follows `../project-ardent/apps/web`: React 19, react-router, strict TS (`verbatimModuleSyntax`,
  `erasableSyntaxOnly`, `noUnused*`), prettier (120 cols, trailing commas), eslint with type-only imports and no
  `for`/`while` loops. No i18n: WebControl is English-only
- Tests colocated as `*.test.ts(x)`; Testing Library + user-event + jest-dom; query by role and visible text
- Dialogs are client-owned React state, built on `app/Dialog`; a screen of its own is a route modal in
  `app/modals.ts`, closed with `useCloseModal()`. jsdom lacks `showModal()`, so `test/dialogPolyfill.ts` stands in
  (`topLayer()` gives the stacking order). Never use `requestPage`/`activateModal` HTML from the server, and never
  round-trip dialog close through the server (`modalClosed` broadcasts `closeModals` to every client)
- Data comes as JSON (`/api` or typed socket messages), never server-rendered HTML fragments
- Plain CSS with the tokens in `theme.css`; no CSS framework. One responsive layout, no `_mobile` variants
- Accessible markup: real `<button>`s with visible text. An icon button needs an `aria-label` and visible text where
  space allows
- Most server pushes are broadcasts to every client (`settingRequest` answers, `checkForGCodeUpdate`, alarms); only
  messages with a `client` field are per-socket
- The serial transcripts and `fixtures/` are specs: change them by hand, only together with the behaviour they pin,
  and never by pasting in what the code now does
