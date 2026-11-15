# WebControl frontend ↔ backend contract

This directory freezes what the React UI (`frontend/`, served at `/next`) and the backend say to each other, in
language-neutral JSON Schema (draft 2020-12). A replacement backend (the planned Rust one) implements it; the Python
backend is checked against it.

Everything except this README and `VERSION` is generated from the zod schemas in `frontend/src/protocol/`. Do not edit
generated files by hand.

## Layout

| Path                                  | What                                                                                 |
| ------------------------------------- | ------------------------------------------------------------------------------------ |
| `VERSION`                             | Contract version, a whole number                                                     |
| `index.json`                          | Manifest: namespaces, events, message commands, actions, HTTP endpoints → schema files |
| `schemas/socket/envelope.json`        | The `message` envelope every server push travels in                                  |
| `schemas/messages/<command>.json`     | The decoded `data` of each server → client `message` command                         |
| `schemas/events/<event>.json`         | The payload of each client → server socket event                                     |
| `schemas/actions/<command>.json`      | The `action` event payload for each `processAction` command, with its `arg`/`arg1`   |
| `schemas/http/<slug>.request.json`    | An endpoint's request body (JSON, or the fields of a multipart form)                 |
| `schemas/http/<slug>.<status>.json`   | An endpoint's response body for one status code                                      |
| `schemas/http/<slug>.params.json`     | Path parameters, for paths with `{name}` placeholders                               |
| `schemas/http/<slug>.query.json`      | Query parameters                                                                     |
| `schemas/logs/message.json`           | `/MaslowCNCLogs` pushes                                                              |
| `schemas/payloads/*.json`             | What the gzip payloads of `gcodeUpdateCompressed` / `boardCutDataUpdateCompressed` inflate to |
| `schemas/websocket/frame.json`        | One frame of the plain-WebSocket transport (below)                                   |

In `index.json`, `actions.<command>.allowedDuring` is the run-state gate and `via` says how the UI reaches it (`action`
event, `opticalCalibration` event, `POST /api/system/pid-test`, or `none`). The gates, by the run state they refuse in:

| Run state                                                         | `always` | `idleOrPaused` | `idleOrControllerPaused` | `idle`  | `noJob` |
| ----------------------------------------------------------------- | -------- | -------------- | ------------------------ | ------- | ------- |
| Streaming gcode                                                   | runs     | refused        | refused                  | refused | refused |
| Paused with Pause                                                 | runs     | runs           | refused                  | refused | refused |
| Job paused by the controller (M0/M1, `Message:`, ALARM)           | runs     | runs           | runs                     | refused | refused |
| Waiting for a tool change outside a job                           | runs     | runs           | runs                     | runs    | refused |
| No job                                                            | runs     | runs           | runs                     | runs    | runs    |

A controller pause holds the job like Pause does; only Play (`startRun`, the one `idleOrControllerPaused` action)
runs, and it resumes the job. A tool change during a job is a controller pause. `noJob` matches the classic gcode
routes' 409: the loaded gcode, home and PID tests wait until the job is stopped or finished. Two firmware pauses
also refuse the machine commands among the `idleOrPaused` actions (jogs, Z moves, home, macros) with an Alert until
Resume or Stop: one that arrived while lines were streaming, which may have job lines buffered behind it, and the
`$16=0` prompt to adjust Z by hand. While a `G38.2` probe (Touch Z or typed) is queued or descending, every action
but `stopRun`, `stopZ`, `pauseRun`, `createDirectory`, `toggleCamera`, `statusRequest` and `queryCamera` is refused
with an Alert, whatever its gate, and so is `/sendGCode` (500); the probe's `ok` or `error:`, Stop, or a reconnect
ends it. HTTP `family` is `api` (JSON for this UI), `classic` (old form routes
the UI reuses, with their old shapes) or `static`.

Schemas describe the wire: what is actually sent, before the UI coerces anything. Objects do not forbid extra
properties; ignore unknown fields. A backend still sends none: `tests/test_contract.py` fails on a field the
schema does not list, so a new one goes into `frontend/src/protocol/` first. Fields marked as numbers-or-numeric-text really do arrive both ways.

## Regenerating and checking

```sh
cd frontend
npm run contract     # rewrites contract/index.json and contract/schemas/
npm test             # src/contract/build.test.ts fails if contract/ is stale
../.venv/bin/python -m pytest ../tests/test_contract.py
```

- `frontend/src/contract/build.test.ts` regenerates in memory and compares with these files.
- `frontend/src/contract/usage.test.ts` records every request the UI's api modules make and fails if one is missing
  from `index.json` or its body does not fit the request schema, or if `index.json` lists an endpoint the UI never
  calls.
- `tests/test_contract.py` checks that Flask registers every endpoint and socket event listed, that
  `Actions.processAction` handles exactly the listed actions and refuses each in exactly the run states its gate
  lists, and validates real GET responses, successful writes, 4xx bodies, every `message` UIProcessor emits and
  what a real gcode file's gzip payloads inflate to against the schemas.

## Versioning

Bump `VERSION` (then `npm run contract`) on any breaking change: removing or renaming a command, event, action,
endpoint or field; changing a type; making an optional field required; accepting less than before; or sending
something the current UI would reject. Additions the UI can ignore (a new optional field, a new command or endpoint)
do not bump it.

## Socket.io transport (today)

Python serves socket.io v4 (EIO=4) at `/socket.io`, long-polling only. Namespaces:

- `/MaslowCNC`: the UI's connection. Client → server events are in `clientToServer`; `classicOnlyClientToServer`
  lists events only the classic UI at `/` sends, which a backend for this UI need not handle.
- `/MaslowCNCLogs`: a second, separate connection for the log viewer. The server pushes `message` events whose
  payload is the log object itself (no envelope); clients send nothing.
- `/WebMCP`: the Docker watchdog's channel (`checkInRequested` → `checkIn`, `shutdown`, `webcontrolMessage`). Not used
  by either UI.

Every `/MaslowCNC` push is a `message` event carrying `{"command": ..., "data": ..., "dataFormat": ...}`.
`dataFormat` is `"json"` for every command except those listed with `"int"` or `"base64"` in `index.json`; with
`"json"`, `data` is JSON **text** that decodes to the command's schema.

Pushes are broadcast to every client. A payload with a `client` field is meant only for the socket with that id
(`"all"` means everyone); clients drop the rest. A connection's id is the socket.io sid.

On connect the server broadcasts `hostAddress`, then the client calls `GET /api/state` and emits
`settingRequest {"section": "Computed Settings", "setting": "units"}` (the server resends the sled position only
after that).

## Plain WebSocket transport (for the Rust backend)

The same events and payloads, without socket.io. Before opening any connection the frontend reads `GET /api/state`
and uses this transport (`frontend/src/transport/websocket.ts`) when `backend` is `"rust"`, socket.io otherwise
(`frontend/src/transport/select.ts`). After three failed connects in a row it reads `/api/state` again and reloads
the page if the backend changed.

- **Endpoint**: `ws://<host>/ws?ns=<url-encoded namespace>` (`wss:` under https), one WebSocket per namespace. The UI
  opens `/MaslowCNC` and, while the log viewer is open, a second one for `/MaslowCNCLogs`.
- **Origin**: the server answers 403 to an upgrade whose `Origin` header is not `http(s)://<Host>`, as socket.io's
  same-origin default does, so another site's page cannot drive the machine.
- **Frames**: UTF-8 text frames only, one JSON object per frame, `{"ns": "/MaslowCNC", "event": "...", "data": ...}`
  (`schemas/websocket/frame.json`). `data` is exactly the single argument socket.io would carry: the envelope for
  `message`, `{"data": {...}}` for client events. No acks, no batching, no binary frames.
- **Binary**: payloads socket.io sends as binary attachments (`gcodeUpdateCompressed`,
  `boardCutDataUpdateCompressed`) are sent as base64 text in the envelope's `data`, still with `"dataFormat":
  "base64"`. `""` or `null` means nothing is loaded.
- **Handshake**: the server's first frame is `{"ns": <ns>, "event": "connect", "data": {"sid": "<id>"}}`. Until it
  arrives the client is connecting and sends nothing. The sid is unique per connection and is what a payload's
  `client` field names.
- **Per-client messages**: a reply meant for the client whose socket event caused it is sent only on that client's
  socket and carries `"client": "<sid>"`. HTTP requests carry no sid, so an HTTP request's answer goes in its response,
  never as a per-client push. Broadcasts go to every socket of the namespace; a broadcast may still carry another
  client's sid, which receivers drop.
- **Unknown input**: either side ignores frames with an unknown `event` or that fail to parse.
- **Liveness**: the server pings every 25 s and closes a connection that has not answered within 20 s (browsers
  answer pings themselves). The client treats a close as a disconnect.
- **Reconnect**: the client reconnects on any close it did not ask for, with exponential backoff: first attempt after
  1 s, doubling to a 5 s cap, each delay randomised by ±50%, forever. A reconnect gets a new sid; the client then
  re-syncs as on first connect (`GET /api/state`, `settingRequest` for units, and the screen's own GETs).
- **Lifecycle events** `connect`, `disconnect` and `connect_error` are local to the client and never sent as frames,
  except the server's `connect` handshake above.

## Known warts

Shapes the backend sends today that a schema can only approximate. A new backend must send them exactly as described
until a `VERSION` bump changes them.

1. **Double-encoded messages.** With `dataFormat: "json"`, envelope `data` is a JSON string, e.g.
   `{"command": "healthMessage", "data": "{\"cpuUsage\": 12.5, \"bufferSize\": 127}", "dataFormat": "json"}`.
2. **Triple-encoded PID results.** `updatePIDData` decodes to `{"command": "updatePIDData", "data": "<JSON text>"}`,
   where the inner text is `{"result": "velocity"|"position", "version": "1"|"2", "data": [...]}`. Version 1 data is
   numbers; version 2 is strings like `"12.0,11.5,30"`. The UI also accepts `data` as the object itself.
3. **HTML in `activateModal`.** `message` is plain text for alerts and notifications (`modalType` `"alert"` /
   `"notification"`, `resume` `"false"`/`"resume"`, `progress` `"false"`/`"spinner"`, `modalSize` `"small"`), but an
   HTML fragment when it answers the classic UI's `requestPage` (then with `isStatic` and the page's own `modalSize`,
   `modalType` and `resume`, plus `client` set to the requester's sid, yet broadcast). This UI never sends `requestPage` and shows only the text kinds.
4. **Errors are alerts.** `processAction` never answers. A refused or failed action broadcasts an `activateModal`
   with `modalType: "alert"`, e.g. `"Cannot issue command while sending gcode."`; an unknown command alerts
   `"Function not currently implemented.. Sorry.[<command>]"`.
5. **Numbers as text.** `homePositionMessage` `xval`/`yval` are numbers after a define-home but config strings like
   `"12.5"` after a `homePosition` settingRequest. `boardDataUpdate` dimensions can be strings. `requestedSetting`
   `value` is a string, number or boolean depending on the setting.
6. **Sentinels.** `connectionStatus.port` is `"none"` when disconnected; `errorValueMessage.computedX/Y` are
   `-999999` when there is no forward-kinematics position; `healthMessage.bufferSize` is `-1` when buffering is off;
   `positionMessage.pcom` is text like `"12.3%"`; `updatePorts` is `["None"]` when no port is found.
7. **Board cut data.** `boardCutDataUpdateCompressed` inflates to a flat array of booleans, one per square inch, row
   by row from bottom left. A board file saved by an old WebControl, whose `boardProcessGCode` stored `[x, y, 0]`
   convex-hull vertices, still loads that way, and marking cuts on such a board mixes the two.
8. **Classic form routes.** They answer `{"status": 200}` / `{"status": 500}` with the HTTP status repeated in the
   body, or a bare JSON string `"success"` / `"failed"` (`/editGCode`, `/sendGCode`, `/editBoard`, `/trimBoard`). A
   409 also puts the message in the reason phrase: `409 Cannot load gcode while a job is running or paused. Stop the
   job first.` A missing form field gives Werkzeug's HTML 400; a library path outside the library gives
   `{"status": 400, "error": "Path is outside the library"}`.
9. **HTML 404s.** `GET /api/settings/<unknown>` and a missing help asset return Flask's HTML 404 page, unlike other
   `/api` errors, which are `{"error": "..."}`.
10. **Accepted is 202.** `POST /api/system/pid-test` answers `202 {"status": "sent"}`; the results arrive later as
    `updatePIDData`.
11. **Stray emits.** On connect the server also emits `my response` `{"data": "Connected", "count": 0}` on the
    default namespace `/` (nobody listens), and the log namespace's connect emits it on `/MaslowCNCLog` (sic). The
    classic UI's screens also receive a top-level `closeModals` event (not a `message`) with `{"data": {"title":
    "Actions"}}`. None of these matter to this UI.
12. **Broadcast answers.** `settingRequest` answers with a `requestedSetting` broadcast, and `checkForGCodeUpdate` /
    `checkForBoardUpdate` resend the gcode or board to every client, not just the asker. Every `action` event, refused
    or not, is followed by the same board resend (`boardDataUpdate`, `showFPSpinner`, `boardCutDataUpdateCompressed`).
13. **Action args.** Commands without arguments still receive `arg` and `arg1` (the UI sends `""`). `moveTo` needs a
    JSON number `arg` (a string fails the bed-bounds check); `move`/`moveZ` parse `arg1` with `float()`, so numeric
    text works too; `defineHome` with both args `""` uses the sled position.
14. **Gcode load signal.** `/openGCode`, `/uploadGCode` and `/editGCode` broadcast `showFPSpinner` with `0` as the
    load starts, then answer the HTTP request once the preview is built. A failed load broadcasts the current
    `gcodeUpdateCompressed`, so every spinner ends; a successful one relies on the client's `checkForGCodeUpdate`
    (`/editGCode` resends it itself). A load waits for one already running, and `startRun` is refused meanwhile;
    a load that finishes after a job has started is dropped with a 500 and the job keeps its gcode. The board resend
    after any `action` (wart 12) ends the spinner early too.

## Rust-intended differences

Where the Rust backend deliberately does not copy Python, as the owner decided (`spec/RUST-PLAN.md`).

- **Every settings import recomputes.** After `/importFileWCJSON` changes anything, Rust runs `computeSettings`
  (`kinematicsTypeComputed`, `distPerRot`, the PID gains, `chainOverSprocketComputed`, `fPWMComputed`), writes the
  derived values that changed to the controller, then sends `$$`, as both backends already do after `/importFile`
  and `/importRestoreWebControl`. Python's `importWebControlJSON` keeps the imported computed values as they are.
- **Slow answers come after the push they used to precede.** Rust solves `errorValueMessage.computedX/Y` and works
  out `gcodePositionMessage` off the machine task, so neither delays Stop. Each push goes out when its work finishes,
  after pushes the same event made at once. While a solve runs, only the newest report waits and the others are
  dropped. Rust solves only for reports that go out. Python solved on every status line, so in quadrilateral mode the
  solver starts from a different tilt and computed X/Y can differ from Python's by the solver's tolerance.
