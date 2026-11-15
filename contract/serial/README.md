# WebControl backend ↔ controller serial contract

This directory freezes what the backend writes to the Maslow controller (holey firmware 51.27 unless a transcript
says otherwise) and how it reacts to what the controller says. A replacement backend (the planned Rust one) must
produce the same bytes; the Python backend is checked against it by `tests/test_serial_contract.py`. The browser side
is in `../README.md`; this contract reuses its action names, HTTP endpoints and `message` commands.

`spec/firmware.md` is the reference for the firmware's behaviour. A transcript records what the backend does today,
which is not always what the firmware needs; the known gaps are listed under "Known warts" below.

## Layout

| Path                        | What                                                                                   |
| --------------------------- | -------------------------------------------------------------------------------------- |
| `transcript.schema.json`    | JSON Schema (draft 2020-12) for one transcript                                         |
| `transcripts/<name>.json`   | The corpus, one behaviour per file, named by behaviour                                 |
| `gcode/<name>.nc`           | G-code files transcripts load by path                                                  |
| `recording.schema.json`     | One line of a serial recording (`~/.WebControl/serial-recordings/*.jsonl`)             |

Tools, in the repository root:

| Tool                                  | What                                                                          |
| ------------------------------------- | ----------------------------------------------------------------------------- |
| `tools/serial_scenarios.py`           | The scenarios the corpus is recorded from: setup plus steps, without the tx   |
| `tools/record_serial_transcript.py`   | Runs scenarios against the real backend and writes `transcripts/`             |
| `tools/convert_recording.py`          | Turns a real-machine recording (and its G-code) into a transcript skeleton    |
| `tools/import_v094_log.py`            | Splits a v0.94 `log.txt`/`alog.txt` into recordings, one per connect          |
| `tests/serial_contract.py`            | The Python replay engine (also used by the recorder)                          |

## A transcript

```json
{
  "formatVersion": 1,
  "meta": {
    "name": "jog_each_direction_mm",
    "description": "Each of the eight jog buttons in mm ...",
    "firmware": { "variant": "holey", "version": "51.27" },
    "webcontrolCommit": "c859cea"
  },
  "setup": { "units": "MM", "home": { "x": 0, "y": 0 } },
  "events": [
    {"type": "ui", "action": {"command": "move", "arg": "up", "arg1": "10"}},
    {"type": "tx", "bytes": "G91  \n"},
    {"type": "rx", "line": "ok"},
    {"type": "tx", "bytes": "G00 Y10.0  \n"},
    {"type": "rx", "line": "ok"},
    {"type": "tx", "bytes": "G90  \n"},
    {"type": "rx", "line": "ok"},
    {"type": "expect", "state": {"positioningMode": "G90", "uploadFlag": 0}}
  ]
}
```

### `meta`

- `name` equals the file name. `description` says what the transcript pins down.
- `firmware.variant` (`holey`, `stock`, `optical`) and `firmware.version` as `B05` reports it (`"51.27"`; every
  51.29x build reports `"51.29"`). `firmware.build` narrows it: `"51.292"` means every `$n=` write also prints
  `Change in setting, rewriting EEPROM`, which is how the backend tells 51.292 from 51.291.
- `webcontrolCommit`: the Python commit the transcript was recorded from.

### `setup`

Applied before the backend starts, in this order:

1. `settings`: values by `webcontrol.json` section and key, e.g. `{"Maslow Settings": {"bufferOn": 1}}`, stored
   without side effects (nothing is sent to the controller for them).
2. `units` (`MM` default, `INCHES`) and `unitsZ` (the Z panel's units, default `units`).
3. `home` `{x, y}` in machine units: `homeX`/`homeY`, the shift added to every absolute X and Y streamed from the file.
4. `gcode`: the loaded file, either `{"path": "gcode/x.nc"}` (relative to this directory) or `{"lines": [...]}`
   (joined with `\n`, ending with one, as a real file). It is loaded the way `/openGCode` loads a file, so the tx
   bytes include the loader's normalisation: every line gains a trailing space, runs of spaces collapse, a space is
   put between a number and the next word.
5. `start`:
   - `"boot"`: the port has just opened (the controller resets on open) and has said nothing; the events start
     with the boot banner.
   - `"connected"` (default): the runner first plays the **prelude** and checks nothing during it.

**The prelude** brings the backend to "connected, handshake done, controller idle and in G90 and the setup units":

1. rx each line of `setup.bootBanner`, default `PCB v1.6 TLE9201  Detected`, `FAKE_SERVO off`, `Grbl v1.00`, `ready`,
   `ok`.
2. Then, until the backend writes nothing more: answer every written line with `ok`, except that `B05` is answered
   `Firmware Version <meta.firmware.version>` then `ok`, and on a 51.292 build every `$n=` write is answered with the
   rewrite notice then `ok`. Each answer is delivered with the same settle rule as an `rx` event (below).
3. If `setup.sled` `{x, y, z}` is given, rx the status report
   `<Idle,MPos:x,y,z,WPos:0.000,0.000,0.000>` (two decimals).
4. If `setup.gcodeIndex` is non-zero, send the `moveGcodeGoto` action with that index (the initial job state:
   Play starts there).

After the prelude everything written so far is forgotten and every UI message emitted so far is discarded.

### `events`

Every event has a `type` and may have a `comment`, which runners ignore.

| `type`   | Fields                                         | The runner                                                                 |
| -------- | ---------------------------------------------- | -------------------------------------------------------------------------- |
| `ui`     | `action: {command, arg, arg1}`                 | sends the socket event `action` `{"data": action}` on `/MaslowCNC`         |
| `http`   | `method`, `path`, `json` or `form`, `status?`  | makes the request; checks the status if given                              |
| `rx`     | `line`                                         | sends `line` followed by `\r\n` from the controller                        |
| `tx`     | `bytes`, `realtime?`, `timeout?`               | checks that the backend's next write is exactly `bytes`                    |
| `tick`   | `seconds`                                      | advances the clock by at least `seconds`                                   |
| `expect` | `state`, `settings`, `emitted`, `notEmitted`, `alerts`, `noAlerts` | checks observable state (below)        |

`tx.bytes` is one write, byte for byte, including trailing spaces and the `\n` (most lines end in two spaces: the
file loader's and the sender's). `realtime: true` marks the realtime commands `!` (stop) and `~` (resume), which
the firmware acts on as soon as it reads them, outside its line buffer and its 126-byte accounting; it is set
exactly when the write is `!` or `~` (today written as `!\n` and `~\n`). Every other write goes through the
buffer.

`expect`:

- `state`: any of `uploadFlag` (`1` streaming, `2` paused by Pause, `-1` stopping after an M0/M1 or tool-changing M6
  line, `0` not streaming: idle, or a job the controller holds), `jobInProgress`, `gcodeIndex` (next file line to
  send), `units`, `manualZAxisAdjust`, `pauseButtonSetting` (`Pause`/`Resume`), `currentTool`, `positioningMode`
  (`G90`/`G91`), `firmwareBuild`, `controllerFirmwareVersion`, `controllerBoard`, `home` `{x, y}`. These are the
  names `GET /api/state` and the `statusMessage` push use, where they exist.
- `settings`: `[{section, key, value}]`, compared as text with `webcontrol.json`.
- `emitted` / `notEmitted`: `/MaslowCNC` `message` pushes, `{command, data}`, where `data` is the decoded payload.
  Objects match field by field, strings as substrings, everything else by equality. Only messages emitted since the
  previous `expect` (or the prelude) count.
- `alerts` / `noAlerts`: shorthand for `activateModal` with `modalType: "alert"` and the given `title`/`message`
  substrings.

## Matching and timing

The runner owns the clock and advances it in small steps (the Python backend sleeps 10 ms per serial loop pass and at
least 50 ms between writes, all on the fake clock). Events run strictly in order:

1. **tx** events match writes in order. The runner waits up to `txTimeout` (default 2 s) for the next write; a
   different write fails at once.
2. Before every other event the runner **settles**: it waits until the backend has read every rx line delivered so
   far and then lets `quiet` (default 0.1 s) pass. Any write that no earlier tx event matched, made at any time
   before the settle ends, fails the transcript as an *unexpected tx*. This is what catches a line sent before the
   ok that should release it, a duplicate, or a stray poll: the backend sends nothing unprompted (no `?` polling;
   the firmware reports status on its own).
3. After the last event the runner settles for `settle` (default 1 s) and fails on any further write.
4. `tick` waits at least `seconds`; writes made during a tick are matched by the tx events after it.

So the timing tolerance is: a write may come any time between the previous event and its tx event (within
`txTimeout`), but never before the controller line or action that causes it. Timing the backend itself depends on
is spelled out with `tick`: after `!`, an ALARM or `Buffer overflow!` the backend ignores acks until it reads a status
report after 0.25 s with no input, so those transcripts `tick` 0.3 s and then send a status report. The backend
treats 5 s without any controller line as a lost connection, so no transcript waits that long without an rx.

Recording uses the same rules, so a recorded transcript replays: the recorder's controller model answers only once
the backend has been quiet for `quiet` seconds, one line per settle.

## The corpus

| Transcript | Pins down |
| --- | --- |
| `connect_boot_handshake` | boot banner → `G21`, `$$`, `G90`, `B05`, `$13` once; B05 reply parsed |
| `connect_settings_sync_adopts_unknown_keys` | `$$` reply: `$18`/`$9`/`$28`/`$36` adopted, a differing `$3` rewritten, `$45` at 8 decimals left alone |
| `connect_eeprom_read_fail_restores_settings` | `error: EEPROM read fail` at boot → every key written back, `$$`, alert |
| `jog_each_direction_mm`, `jog_each_direction_inches` | the eight jog buttons |
| `home_at_safe_height_mm`, `return_to_center_inches` | Home and Return to Center, safe height in each unit |
| `define_home_from_sled_then_home` | Define Home from the sled and from arguments (inches → mm); Home goes there |
| `move_z_raise_lower_and_zero_mm`, `move_z_in_inches_on_a_mm_machine` | Z moves, `G10 Z0`, the Z panel's own units |
| `touch_z_mm`, `touch_z_inches` | `G38.2` probe then `G10 Z0` |
| `stream_unbuffered_with_home_shift` | one line per ok; home shift keeps both operands' decimals |
| `stream_buffered_short_job_fits_the_buffer` | the same job buffered: 125 bytes, all sent before the first ok |
| `stream_buffered_with_home_shift` | 126-byte character counting, refilled by each ok |
| `start_mid_file_restores_modal_state` | restart sequence: safe height, XY, `G1` plunge at the plunge feed, `G1 F<file F>` |
| `pause_jog_resume_returns_to_file_position` | Pause with a line in flight, jog while paused, Resume returns and replunges |
| `m0_in_file_then_resume` | M0 stops streaming; Resume sends `~` and returns |
| `tool_change_t_and_m6_on_one_line` | `T2 M6`: stop after it, prompt, Resume returns at safe height without plunging |
| `tool_change_t_then_m6_on_separate_lines` | `T2` / `M6` → the M6 goes out as `T2 M6` |
| `tool_change_m6_before_t_is_rewritten` | `M6 T2` goes out as `T2 M6` |
| `tool_change_jog_and_zero_z_then_resume` | a Z move releases the firmware once; `G10 Z0`; Resume |
| `stop_mid_line_resets_the_index` | `!` with a line in flight; index reset; commands wait for a status report |
| `stop_with_a_stale_ok_in_flight` | a late ok after `!` does not release the next command early |
| `alarm_mid_cut_rewinds_and_resumes` | ALARM: halt, rewind to the oldest unacknowledged line, Play resumes there |
| `buffer_overflow_halts_streaming` | `Buffer overflow!`: halt, rewind, Stop |
| `line_too_long_pauses_the_job` | a line over 126 bytes as written is never sent; the job pauses with an alert |
| `unclosed_comment_pauses_the_job` | an unclosed `(` is never sent; the job pauses with an alert |
| `realtime_characters_stripped_from_lines` | `!`, `~`, backtick, `?`, control characters and comments removed from lines |
| `z_limits_refused_on_51_27` | Z-limit actions alert and send nothing below 51.292 |
| `z_limits_on_51_292` | `$63`/`$64` in mm, clear, `B20` |
| `holey_calibration_accept_writes_settings` | Calculate sends nothing; Accept writes `$3`, `$2`, `$40`, `$41` |

## Adding or changing a transcript

1. Add or edit a scenario in `tools/serial_scenarios.py`. Steps are events without tx, plus recorder-only steps:
   `idle()` (the controller model answers until the backend goes quiet), `wait_tx(regex)` (answer until the backend
   writes a matching line, which stays unanswered), `firmware(ack=, flush=, forget=, rule=)` and
   `pauses_on(regex, *replies)` (a line the firmware holds until `~`, after printing `replies`).
2. `.venv/bin/python tools/record_serial_transcript.py <name>` (no name: all of them).
3. Read the result against `spec/firmware.md` before committing. The recorder records whatever the backend does.
4. `.venv/bin/python -m pytest tests/test_serial_contract.py`.

Transcripts are written one event per line; the test fails on any other layout, so always write them with the
recorder or `serial_contract.dumps`.

A recording from a real machine becomes a transcript with `tools/convert_recording.py` (below); a v0.94 log first
goes through `tools/import_v094_log.py`.

## Versioning

`formatVersion` changes only when the transcript format does. A behaviour change in the backend changes the
transcripts it affects, in the same commit, re-recorded and re-checked.

## How the Rust backend consumes this

The Rust backend gets its own runner, which implements the replay rules above against the Rust backend with a fake
serial port and a virtual clock. The format is plain JSON designed for `serde`:

- Events are an internally tagged enum: `#[serde(tag = "type", rename_all = "lowercase")] enum Event { Ui { action:
  Action }, Http { .. }, Rx { line: String }, Tx { bytes: String, #[serde(default)] realtime: bool, timeout:
  Option<f64> }, Tick { seconds: f64 }, Expect(Expect) }`, each with `#[serde(default)] comment: Option<String>`.
  `Action.arg`/`arg1` are strings or numbers (`serde_json::Value`, or an untagged enum).
- `setup.gcode` is an untagged enum of `{lines}` or `{path}`. Unknown fields are not allowed anywhere
  (`deny_unknown_fields` is safe), so a typo fails loudly.
- Every number fits `f64`. `tx.bytes` and `rx.line` are UTF-8; in practice ASCII.

The runner needs these seams in the backend:

1. A serial port trait the runner can implement: the backend writes byte strings and reads lines. Compare each
   `write` call against the next `tx`. If the Rust backend batches several lines into one write, split on `\n`
   before comparing (each tx is one line); a `realtime` tx must not be batched behind buffered lines.
2. A clock trait (or `tokio::time::pause()` with `advance`) so settles and ticks cost no real time. Advance in steps of
   at most 10 ms so the backend's own timers (write pacing, the 0.25 s resync, the 5 s link timeout) see time pass
   as the Python one does.
3. A way to know the backend has consumed every rx line (an idle signal, or "the read channel is empty and the
   backend task is parked"), for the settle rule.
4. Settings, units, home and the gcode file set before start, the same way the app loads them; the `ui` events go
   through the same handler the WebSocket `action` frame reaches, and `http` events through the router.
5. `expect.state` names map onto `GET /api/state` and `statusMessage` fields; `emitted` checks the `message` pushes
   the WebSocket transport would send (decoded `data`).

Rust does not need to reproduce the prelude's tx: it only has to reach the same state from the same rx lines. It
must reproduce every byte after it. Where this contract pins something `spec/firmware.md` says is wrong (below), the
Rust backend may fix it only together with a re-recorded transcript and a note here.

## Recordings from a real machine

With **WebControl Settings → Record serial traffic** on, the Python backend appends every serial line to
`~/.WebControl/serial-recordings/<session>.jsonl`, a new session per connect. Each line is one JSON object
(`recording.schema.json`) with `t`, monotonic seconds since the session started, and `type`:

| `type`    | Fields                                                     |
| --------- | ---------------------------------------------------------- |
| `session` | `started` (ISO wall time), `webcontrol`, `port`, `units`, `home`, `settings` (Maslow and Advanced sections, key → value); `source` for an imported log |
| `tx`      | `data` (the exact bytes written), `cause` (the `seq` of the most recent UI action, if any) |
| `rx`      | `data` (the line as read, with its `\r\n`)                 |
| `ui`      | `seq`, `command`, `arg`, `arg1`                            |
| `gcode`   | `filename`, `lines` (a file was loaded)                    |
| `dropped` | `count` (lines lost because the writer fell behind)        |

Writing happens on a background thread from a bounded queue: a full queue drops lines and counts them, so recording
never slows streaming. Files rotate at 5 MB (`<session>.jsonl`, `<session>.1.jsonl`, ...) and the directory keeps the
newest 40 files.

`tools/convert_recording.py <recording.jsonl>... [--gcode file.nc] [--name name]` writes a transcript skeleton to
`transcripts/` (and copies the G-code into `gcode/`):

- setup from the `session` line: start `boot` (a session begins at the connect), its units and home, and only the
  settings that differ from `defaultwebcontrol.json`;
- `ui`, `rx` and `tx` events in order, with `tick` for every gap of 0.25 s or more (capped at 4.5 s, under the 5 s
  link timeout);
- a `TODO` comment on the first write of any burst that follows a second or more without writes and no recorded action
  (every such burst in a v0.94 log), and a comment where the recorder dropped records;
- the firmware from the `B05` reply (and 51.292 from its rewrite notice).

Status reports are kept (they end resyncs) unless `--drop-status`. Expect events are not invented; add them by hand,
trim, then replay. `tests/test_convert_recording.py` records sessions through the backend recorder, converts them and
replays the result.

## v0.94 logs

v0.94 never truncated `~/.WebControl/log.txt` and `alog.txt` (its logger opened them with `"a"` and called
`truncate()`, which truncates at the end), so they hold months of traffic. The format, from `DataStructures/logger.py`
and `DataStructures/loggingQueue.py` at fe1f113:

```
+++++
2024-03-02 09:14:55          <- each WebControl start: a header with the local start time
+++++
0.53: Sent: G21              <- seconds since start, two decimals; every write, rstripped of \r\n
0.61: ok                     <- every controller line, as read (message_queue puts are logged)
0.82: <Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>   <- log.txt only
1.01: Ground Control Version 0.94
15.20: Logging suspended due to user idle time > 60 seconds   <- log.txt only
```

- `log.txt` has everything; status (`<`) and `[PE:...]` lines are only there, and it stops while logging is
  suspended (`loggingTimeout`). `alog.txt` has every non-status line, suspended or not.
- `Sent: ` lines end with the backend's trailing space(s); the newline it wrote is not in the log (it always is one
  `\n`). `Send FAILED: ` marks a write that raised.
- Lines are buffered and flushed in batches by a thread, so a crash loses the last few.

`tools/import_v094_log.py log.txt [--alog alog.txt] --out DIR` splits the log at every `+++++` header and at every
connect, and writes one recording per connect, named by its start time, for `convert_recording.py`:

- v0.94 sends `B05`, the units and `$$` when it opens the port, so a connect starts at each `Sent: B05`, or at the
  boot banner (`PCB v1.`, `Grbl v1.00`, `ready`) up to 10 s before it. Lines before a process's first connect are
  dropped.
- `Sent: X` becomes a tx of `X\n`; any other entry an rx of the line plus `\r\n`. `Ground Control Version` and the
  suspend/resume markers are dropped; a `Send FAILED:` write is left out with a warning.
- The session line gets the units from the first `G20`/`G21` written, and the settings WebControl ended the handshake
  with: each `$n=` value from the `$$` reply, replaced by any `$n=` WebControl wrote, mapped back to setting keys through
  `defaultwebcontrol.json`'s `firmwareKey`. Home and the G-code file are not in the log.
- With `--alog`, entries `log.txt` lacks inside a suspended window are taken from `alog.txt`, matched by the header's
  start time.

UI actions are not in v0.94 logs, so the transcript needs its `ui` events added by hand where the TODO comments are.
v0.94's handshake (`B05` first) also differs from today's, so a converted session is a starting point: keep the part
that shows the behaviour (usually a job), restart it from `start: connected`, and re-record or hand-fix the tx.
`tests/fixtures/v094/` holds a synthetic log and alog in this format; `tests/test_import_v094_log.py` checks the
importer against it.

## Known warts

Behaviour the transcripts pin today that `spec/firmware.md` or plain sense says is off. A Rust backend matches it
until a transcript and this list change together.

1. **Realtime commands carry a newline.** `!` and `~` go out as `!\n` and `~\n`. The firmware acts on the character at
   once; the `\n` then reaches its line buffer as an empty line.
2. **Resume always sends `~`**, even after a plain Pause, when the firmware is not paused. It costs an extra `ok`.
3. **Doubled spaces.** Most lines end in two spaces (`G21  `, `G0 Z5  `); `home` and `returnToCenter` send `G90   `.
4. **Handshake order.** `G21`, `$$`, `G90` go out before `B05` and the `$13` write, so `$$` is compared before the
   firmware version is known.
5. **A lone `T2` line is sent** although the firmware runs it as an empty modal move; the following `M6` is led by
   `T2` anyway.
6. **touchZ** sends `G90 G38.2 Z-<limit> M02 ` and queues `G10 Z0` at once, so Z is zeroed even when the probe
   fails (`error: probe did not connect`), and the `M02` ends the line with a program-end word.
7. **Z moves in the other unit flip the host's units.** The Z panel's `G20`/`G21` (and touchZ's on an inch machine)
   switch WebControl's own units and back, rescaling home by 1/25.4 and 25.4 each time, which drifts it
   (100.3 → 100.29999999999997 after three moves) and broadcasts unit changes.
8. **Setting formats vary.** A restore writes `$16=0.000000`, `$21=1300.00`, `$45=0.00000516850`: whatever each
   setting's formatter produces.
