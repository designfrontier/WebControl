# CLAUDE.md

## Project structure

---

### Key directories

- `.github/workflows/` — CI: frontend lint/typecheck/test/build, backend pytest, Playwright e2e
- `Actions/` — `actions.py` dispatches every UI action (jog, home, run/pause/stop, settings, firmware upgrade, backup); plus triangular, holey, optical calibration and GPIO actions
- `api/` — JSON blueprint mounted at `/api` for the new frontend; every `api/<feature>.py` exposing `blueprint` auto-registers
- `Background/` — long-running worker loops: controller message routing, UI socket push (`UIProcessor`), log streaming, webcam capture, console/WebMCP queue
- `Boards/` — board (workpiece) model and manager tracking dimensions and cut-area grid
- `config/` — `Config` class: loads/saves `~/.WebControl/webcontrol.json`, computes derived settings, syncs settings with firmware
- `Connection/` — serial link to the Arduino (`serialPortThread` buffering/flow control) and `NonVisibleWidgets`, which wires all backend services onto `Data`
- `DataStructures/` — `Data` global state object, inter-thread queues, logger, and `MakesmithInitFuncs` base that injects `data`
- `docs/` — Jekyll help site (also served in-app) for actions, boards, getting started
- `docs/Actions/` — help pages for each UI action
- `docs/Actions/Calibration-Setup/` — help for calibration and machine setup actions
- `docs/Actions/Controller/` — help for firmware upgrade and settings sync
- `docs/Actions/Diagnostics-Maintenance/` — help for diagnostics, logs, backups
- `docs/assets/` — images used by the help pages
- `docs/assets/Actions/` — screenshots for action help pages
- `docs/assets/boards/` — board management screenshots
- `docs/assets/gettingStarted/` — getting-started screenshots
- `docs/assets/holeyCalibration/` — holey calibration diagrams
- `docs/assets/triangularCalibration/` — triangular calibration diagrams
- `docs/Boards/` — board management help
- `docs/GettingStarted/` — first-steps setup guide
- `File/` — G-code file loading/parsing/compression for the 3D preview, and settings import
- `frontend/` — the new React + TypeScript UI served at `/next` (see "New frontend" below)
- `HelpManager/` — stub for fetching updated help pages from GitHub (mostly no-op)
- `ReleaseManager/` — checks GitHub releases and self-updates PyInstaller builds
- `static/` — browser assets served by Flask
- `static/images/` — UI images
- `static/images/gettingStarted/` — setup wizard images
- `static/images/GPIO/` — Raspberry Pi pin layout diagram
- `static/images/holeyCalibration/` — holey calibration pattern images
- `static/images/icons/` — UI icons
- `static/images/materials/` — board material textures for the 3D view
- `static/images/quickConfig/` — quick-configure wizard measurement images
- `static/images/triangularCalibration/` — triangular calibration images
- `static/scripts/` — frontend JS: socket client (`baseSocket.js`), three.js 3D frontpage, per-modal scripts, vendored libs (jQuery, Bootstrap, Ace, socket.io)
- `static/scripts/archive/` — retired 2D SVG frontpage scripts
- `static/styles/` — CSS (Bootstrap + app styles)
- `templates/` — Jinja pages and modal fragments (desktop + `_mobile` variants)
- `templates/archive/` — retired 2D frontpage templates
- `tests/` — backend pytest suite (`conftest.py` boots `main.py` with serial port, camera and network checks faked)
- `tools/` — install/upgrade/repair scripts and OpenCV build helper
- `WebPageProcessor/` — renders modal HTML for each `requestPage` socket event

### Key files

- `app.py` — creates Flask app, SocketIO and Mobility instances
- `docs/index.md` — help site home
- `main.py` — entry point: builds `Data`, starts worker threads, defines HTTP routes and socket handlers, runs the server
- `main.spec` — PyInstaller build spec; it does not bundle `frontend/dist` yet, so packaged builds return 503 at
  `/next`
- `next_ui.py` — serves `frontend/dist` at `/next` with SPA fallback
- `tests/e2e_backend.py` — runs `main.py` on a throwaway `HOME` and port for Playwright

## New frontend

The classic UI at `/` (`templates/`, `static/`) stays and must keep working unchanged. The React port lives in
`frontend/` and is served at `/next`. Backend changes for it are additive only: new `/api` endpoints, new socket
events, per-client emits.

### Layout

- `frontend/src/protocol/` — zod schemas for the socket and API contract, one file per feature (`main`, `viewer`,
  `settings`, `files`, `boards`, `machine`, `calibration`, `system`), each exporting `<x>Messages` (server -> client
  `message` commands), `<x>Actions` (processAction command names) and `<x>Events` (client -> server events);
  `core.ts` holds the envelope; `index.ts` only merges them
- `frontend/src/socket/` — `createClient(socket)`: validates messages, drops ones whose `client` field names another
  socket, `sendAction(command, arg?, arg1?)`, `requestSetting`, `updateSetting`, typed `emit`
- `frontend/src/api/` — `getJson`/`sendJson(fetch, path, schema)` against `/api`
- `frontend/src/features/files/api.ts` — `postForm(fetch, path, fields)` posts FormData to classic `main.py` form
  routes (`/openGCode`, `/trimBoard`, ...); each such route also needs an entry in the `vite.config.ts` dev proxy
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
  (`serverConnect`, `serverMessage(command, data)`, `emitted`, `actions()`), `fakeFetch(routes)`, `startupState()`; `filesBoards.ts` (`expectPath`, `recorder`, `postedForm`, library and board
  fixtures)
- `frontend/src/styles/theme.css` — design tokens (custom properties); `app.css` — shell layout

### Commands

- Backend setup:
  `uv venv --python 3.9 .venv && uv pip install --python .venv/bin/python -r requirements.txt -r requirements-dev.txt`
  (on Apple Silicon add `--override requirements-dev-overrides.txt`; gevent 22.10.2 has no arm64 wheel for 3.9)
- Backend tests: `.venv/bin/python -m pytest` (`--cov=api --cov=next_ui` for coverage)
- Frontend (in `frontend/`, Node 24+): `npm ci`, `npm run dev` (Vite at http://localhost:5173/next/, proxying `/api`,
  `/socket.io`, `/static` to `WEBCONTROL_BACKEND`, default http://localhost:5000), `npm run lint`,
  `npm run typecheck`, `npm test` (Vitest + v8 coverage, 95% floor), `npm run build` (to `frontend/dist`, served by
  Flask at `/next`), `npm run format`
- E2E: `npm run test:e2e` in `frontend/` builds the app and starts `tests/e2e_backend.py` on port 5055 (override with
  `E2E_BACKEND_PORT`; Python from `E2E_PYTHON`, default `../.venv/bin/python`). First time:
  `npx playwright install chromium`. It never touches the real `~/.WebControl`
- Never run `main.py` against the real `~/.WebControl` for testing; use `tests/e2e_backend.py`, which also stops
  `main.py` from opening a browser tab

### Slice ownership

Each slice edits only its own `frontend/src/features/<x>/`, `frontend/src/protocol/<x>.ts`, `api/<x>.py` and
`tests/test_api_<x>.py`, and replaces its placeholder inside its own folder. A slice's `index.ts` only re-exports
its components; logic goes in a sibling module, because coverage skips `index.ts` files. A new modal screen needs an
entry in `app/modals.ts`; `protocol/index.ts` already imports every slice. Other shared files (`app/`, `state/`,
`socket/`, `test/`, `api/__init__.py`, `main.py`) change only for that slice's own placeholder entry, kept minimal
and additive.

New endpoints go in `api/<x>.py` (a module-level `blueprint`, auto-mounted under `/api`). Emit to just the requester
with `api.client_emit.emit_to_client(command, data)`; outside a socket handler, pass `sid=`. A new socket event needs a
`@socketio.on(..., namespace="/MaslowCNC")` handler in `main.py` or `api/`; the protocol contract test fails if an
event has none.

### Conventions

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
- The server pins python-socketio 5.7.2 / python-engineio 4.3.4, which accept only socket.io-client 3/4 (EIO=4);
  there is no WebSocket transport (no gevent-websocket), so the client long-polls
- Most server pushes are broadcasts to every client (`settingRequest` answers, `checkForGCodeUpdate`, alarms); only
  messages with a `client` field are per-socket
