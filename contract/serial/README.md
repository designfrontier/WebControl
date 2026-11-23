# WebControl backend ↔ controller serial contract

This directory freezes what the backend writes to the Maslow controller (holey firmware 51.27 unless a transcript
says otherwise) and how it reacts to what the controller says. The Rust backend must produce the same bytes;
`cargo test -p wc-machine --test transcripts` replays every transcript against it. The corpus was first recorded from
the Python backend (in git history up to `220aa0e`); it is now a hand-maintained spec ("Adding or changing a
transcript" below). The browser side is in `../README.md`; this contract reuses its action names, HTTP endpoints and
`message` commands.

`spec/firmware.md` is the reference for the firmware's behaviour. A transcript records what the backend does today,
which is not always what the firmware needs; the known gaps are listed under "Known warts" below.

## Layout

| Path                        | What                                                                                   |
| --------------------------- | -------------------------------------------------------------------------------------- |
| `transcript.schema.json`    | JSON Schema (draft 2020-12) for one transcript                                         |
| `transcripts/<name>.json`   | The corpus, one behaviour per file, named by behaviour                                 |
| `gcode/<name>.nc`           | G-code files transcripts load by path                                                  |
| `gcode/real/<name>.nc`      | Trimmed G-code from the owner's real jobs ("Real jobs" below)                          |
| `real-jobs/<name>.json`     | What v0.94 sent and read for those lines on the owner's machine                        |
| `real-jobs.md`              | The backend's writes for the real jobs against v0.94's, differences classified         |
| `recording.schema.json`     | One line of a serial recording (`~/.WebControl/serial-recordings/*.jsonl`)             |

The Python tools that made the corpus are in git history up to `220aa0e`, and the sections below that describe them
are the record of how it was made: `tools/serial_scenarios.py` (the scenarios), `tools/record_serial_transcript.py`
(ran them against the Python backend), `tools/convert_recording.py` and `tools/import_v094_log.py` (real-machine
recordings and v0.94 logs into transcript skeletons), `tools/index_recordings.py`, `tools/owner_machine.py` (the
owner's controller lines and settings), the real-job tools, and `tests/serial_contract.py` (the Python replay
engine). The Rust runner is `rust/crates/wc-testkit/src/runner.rs`.

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
- `webcontrolCommit`: the commit the transcript was recorded from (a Python one) or last edited at.

### `setup`

Applied before the backend starts, in this order:

1. `settings`: values by `webcontrol.json` section and key, e.g. `{"Maslow Settings": {"bufferOn": 1}}`, stored
   without side effects (nothing is sent to the controller for them).
2. `units` (`MM` default, `INCHES`) and `unitsZ` (the Z panel's units, default `units`).
3. `home` `{x, y}` in machine units: `homeX`/`homeY`, the shift added to every absolute X and Y streamed from the file.
4. `gcode`: the loaded file, either `{"path": "gcode/x.nc"}` (relative to this directory; one subdirectory deep at
   most) or `{"lines": [...]}` (joined with `\n`, ending with one, as a real file). It is loaded the way `/openGCode` loads a file, so the tx
   bytes include the loader's normalisation: every line gains a trailing space, runs of spaces collapse, a space is
   put between a number and the next word.
5. `start`:
   - `"boot"`: the port has just opened and the controller has said nothing; the events start with the boot banner
     (it resets on open), or without one for a board that doesn't.
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
| `tx`     | `bytes`, `realtime?`, `timeout?`               | checks that the next line the backend wrote is exactly `bytes`             |
| `tick`   | `seconds`                                      | advances the clock by at least `seconds`                                   |
| `expect` | `state`, `settings`, `emitted`, `notEmitted`, `alerts`, `noAlerts` | checks observable state (below)        |

`tx.bytes` is one line of the byte stream the backend writes, byte for byte, including trailing spaces and the
`\n` (most lines end in two spaces: the file loader's and the sender's). Write boundaries don't matter: the
controller sees only bytes, so the runner joins writes and cuts after each `\n`; a line left without its `\n` at the
end fails. It is printable ASCII (`^[ -~]*\n$`), which is all the backend ever sends. `realtime: true` marks the realtime commands `!` (stop) and `~` (resume), which
the firmware acts on as soon as it reads them, outside its line buffer and its 126-byte accounting; it is set
exactly when the write is `!` or `~` (today written as `!\n` and `~\n`). Every other write goes through the
buffer.

`expect`:

- `state`: any of `uploadFlag` (`1` streaming, `2` paused by Pause, `-1` stopping after an M0/M1 or tool-changing M6
  line, `0` not streaming: idle, or a job the controller holds), `jobInProgress`, `gcodeIndex` (next file line to
  send), `units`, `manualZAxisAdjust`, `pauseButtonSetting` (`Pause`/`Resume`), `currentTool`, `positioningMode`
  (`G90`/`G91`), `firmwareBuild`, `controllerFirmwareVersion`, `controllerBoard`, `home` `{x, y}`. These are the
  names `GET /api/state` and the `statusMessage` push use, where they exist.
- `settings`: `[{section, key, value}]`; `value` is a string, compared with the stored value as text (as Python's
  `str()` prints it: `"12.6"`, `"1"`).
- `emitted` / `notEmitted`: `/MaslowCNC` `message` pushes, `{command, data}`, where `data` is the decoded payload.
  Objects match field by field, strings as substrings, lists element by element. Numbers compare as `f64` (`254`
  equals `254.0`) and booleans only equal booleans, as `serde_json::Value` would see them; `state` compares the same
  way. Only messages emitted since the previous `expect` (or the prelude) count, so `notEmitted` covers only that
  window.
- `alerts` / `noAlerts`: shorthand for `activateModal` with `modalType: "alert"` and the given `title`/`message`
  substrings.

## Matching and timing

The runner owns the clock and advances it in steps of at most 10 ms (the Python backend slept 10 ms per serial loop
pass and, before a line due under 50 ms after the last write, a full 50 ms more, which set the timings below; "Known
warts" 8). Events run strictly in order:

1. **tx** events match lines in order. The runner waits up to `txTimeout` (default 2 s) for the next line; a
   different line fails at once, and so does the right line written after the timeout (each line is stamped with
   the fake clock when its `\n` is written).
2. Before every other event the runner **settles**: it waits until the backend has read every rx line delivered so
   far and then lets `quiet` (default 0.1 s) pass. Any write that no earlier tx event matched, made at any time
   before the settle ends, fails the transcript as an *unexpected tx*. This catches a duplicate, a stray poll (the
   backend sends nothing unprompted: no `?` polling; the firmware reports status on its own), and a line sent before
   the ok that releases it, within `quiet` of the write before. A backend that releases lines on a timer longer than
   that is caught by the `*_waits_for_a_slow_ok` transcripts, which hold an ok for 4 s.
3. After the last event the runner settles for `settle` (default 1 s) and fails on any further write; a write later
   than that is not seen.
4. `tick` waits at least `seconds`; writes made during a tick are matched by the tx events after it.

So the timing tolerance is: a write may come any time between the previous event and its tx event (within
`txTimeout`), but never before the controller line or action that causes it. Timing the backend itself depends on
is spelled out with `tick`: after `!`, an ALARM or `Buffer overflow!` the backend ignores acks and holds lines until
it reads a status report once its input has run dry at least 0.25 s after the halt, so those transcripts `tick`
0.3 s and then send a status report; `stop_ignores_a_status_report_read_too_soon` pins that an earlier report
releases nothing. After an ALARM or overflow the resync releases only `$` and modal lines: anything that moves waits
for Stop, or Play on the held job (Rust-intended deviation 4). A line the controller never received goes out again
once three status reports past a 150 ms grace after the write show an empty ring and a still sled with no echo or
ack (Rust-intended deviation 6), so those transcripts stream the recorded reports with `tick` 0.2 between them. The backend treats 5 s without any
controller line as a lost connection, so no transcript waits that long without an rx.

The Python recorder used the same rules, so a recorded transcript replayed: its controller model answered only once
the backend had been quiet for `quiet` seconds, one line per settle.

## The corpus

| Transcript | Pins down |
| --- | --- |
| `connect_boot_handshake` | the owner's real connect (51.27, PCB v1.1, full `$$`): `B05`, `G21`, `G90`, `$$`, then only `$13`; no `$38`; B05 reply parsed |
| `connect_settings_sync_adopts_unknown_keys` | `$$` reply: `$18`/`$9`/`$28`/`$36` adopted, a differing `$3` written with `$13`, then `$$` again; `$45` at 8 decimals left alone |
| `connect_eeprom_read_fail_restores_settings` | `error: EEPROM read fail` at boot → `$$`, every key written back, alert; the factory `$38` differs, so the port is reopened (a reset) and the next boot writes only `$13` |
| `connect_without_a_reset_handshakes_after_3_s` | no banner and no `ok` 3 s after the port opens → the same handshake: `B05`, units, `G90`, `$$`, `$13` |
| `connect_chain_over_sprocket_differs_restarts_the_controller` | the owner's `$$` with `$38=1`: `$13` and `$38` written, the port reopened (a reset), and the next boot writes only `$13` |
| `connect_repairs_a_pid_weight_above_one` | the owner's `$$` with `$24` and `$32` at 1.00000011 (an earlier `$24=1.000000` write): equal at the printed precision but refused by `SetTunings`, so `$13`, `$24=1` and `$32=1` are written, then `$$` is read back |
| `jog_each_direction_mm`, `jog_each_direction_inches` | the eight jog buttons |
| `home_at_safe_height_mm`, `return_to_center_inches` | Home and Return to Center, safe height in each unit |
| `home_refused_at_a_safe_height_of_zero` | Home and Return to Center with `zAxisSafeHeight` 0: refused with an alert naming it, nothing written (Rust-intended deviation 10) |
| `define_home_from_sled_then_home` | Define Home from the sled and from arguments (inches → mm); Home goes there |
| `move_z_raise_lower_and_zero_mm`, `move_z_in_inches_on_a_mm_machine` | Z moves, `G10 Z0`; a Z distance in the other unit is converted, units and home never change |
| `touch_z_mm`, `touch_z_inches` | `G38.2` probe in the machine's units (the mm plunge limit converted); no `G10 Z0`, the firmware zeroes Z on a touch |
| `touch_z_probe_fails_leaves_z_alone` | `error: probe did not connect` acks the probe: no `G10 Z0`, an alert |
| `resume_during_probe_is_refused_until_the_probe_touches` | Resume mid-probe in a paused job is refused with an alert: no `~`, no restart; nothing follows the probe's `ok`; the next Resume returns to the job |
| `resume_during_probe_is_refused_then_the_probe_misses` | the same, and the probe misses: the Touch Z Failed alert, no `G10 Z0` |
| `probe_in_file_waits_for_the_touch` | Buffered streaming waits behind a `G38.2` from the file; a jog is refused and Clear Logs runs mid-probe; Resume after the touch lowers to Z 0 |
| `probe_in_file_misses_and_waits_at_the_probe` | A `G38.2` from the file misses: the Touch Z Failed alert, the job held at the probe line, Play returns and probes again |
| `touch_z_without_zeroing_alerts` | An `ok` with no `z axis zeroed` before it alerts that Z was not zeroed |
| `probe_g38_3_4_5_refused_typed_and_in_file` | Typed G38.3 and G38.5 are dropped with an alert; a G38.4 in the file pauses the job before it (Rust-intended deviation 11) |
| `stream_unbuffered_with_home_shift` | one line per ok; home shift keeps both operands' decimals |
| `stream_buffered_short_job_fits_the_buffer` | the same job buffered: 125 bytes, all sent before the first ok |
| `stream_buffered_with_home_shift` | 126-byte character counting, refilled by each ok |
| `stream_unbuffered_waits_for_a_slow_ok`, `stream_buffered_waits_for_a_slow_ok` | an ok 4 s late (status reports in between) releases nothing early |
| `start_mid_file_restores_modal_state` | restart sequence: safe height, XY, `G1` plunge at the plunge feed, `G1 F<file F>` |
| `resume_above_safe_height_never_lowers_before_travel` | Resume with the bit above the safe height travels at the bit's Z; so does Return to Center |
| `resume_on_51_292_sets_the_spindle_speed_after_m3` | On 51.29x Resume restores the spindle with `M3` then the file's last `S` on its own line |
| `pause_jog_resume_returns_to_file_position` | Pause with a line in flight, jog while paused, Resume returns and replunges |
| `m0_in_file_then_resume` | M0 stops streaming; Resume sends `~` and returns |
| `tool_change_t_and_m6_on_one_line` | `T2 M6`: stop after it, prompt, Resume returns at safe height without plunging |
| `tool_change_t_then_m6_on_separate_lines` | `T2` / `M6` → the lone `T2` is not sent; the M6 goes out as `T2 M6`; Resume sets no feed the file never set |
| `tool_change_m6_before_t_is_rewritten` | `M6 T2` goes out as `T2 M6` |
| `tool_change_jog_and_zero_z_then_resume` | a Z move releases the firmware once; `G10 Z0`; Resume |
| `stop_mid_line_resets_the_index` | `!` with a line in flight; index reset; commands wait for a status report |
| `stop_with_a_stale_ok_in_flight` | a late ok after `!` does not release the next command early |
| `stop_ignores_a_status_report_read_too_soon` | a status report within 0.25 s of `!` does not end the resync |
| `alarm_mid_cut_rewinds_and_resumes` | ALARM: halt, rewind to the oldest unacknowledged line, Play resumes there |
| `alarm_mid_home_holds_the_plunge_until_stop` | The owner's Home with the motor supply off: ALARM mid-travel; Home's `G00 Z0` never goes out, a jog is refused, Stop clears it and the next jog goes |
| `stream_lost_line_is_sent_again` | the owner's cut: an arc never echoed or acked, the ring empty and the sled still for three reports; it goes out again once and the job goes on |
| `stream_line_lost_twice_pauses_until_resume` | the same arc lost again: the job pauses at it with an alert; Resume returns there and sends it |
| `buffer_overflow_halts_streaming` | `Buffer overflow!` and its buffer dump: halt, rewind, Stop |
| `line_too_long_pauses_the_job` | a line over 126 bytes as written is never sent; the job pauses with an alert |
| `unclosed_comment_pauses_the_job` | an unclosed `(` is never sent; the job pauses with an alert |
| `r_arc_converted_to_i_j_on_51_27` | R arcs go to 51.27 with I and J (negative R over 180°, a G91 arc written absolute) |
| `r_arc_impossible_radius_pauses_the_job` | an R shorter than half the chord is never sent; the job pauses with an alert |
| `r_arc_converted_on_51_292` | 51.29x gets R arcs as I and J too, since it drops R's sign |
| `g91_arc_written_absolute` | a G91 I/J arc goes out with the absolute X and Y it ends at; G1 under G91 as written |
| `g91_arc_with_z_pauses_the_job` | a G91 arc with Z (which the firmware takes as absolute) is never sent; the job pauses |
| `g91_1_left_out_of_a_vcarve_file` | a VCarve-style `G90 G91.1` goes out as `G90`, a lone `G91.1` not at all; the moves after it stay absolute |
| `g90_1_arc_centres_written_as_offsets` | G90.1 is never sent; its arcs' centres, shifted to home, go out as I and J offsets |
| `realtime_characters_stripped_from_lines` | `!`, `~`, backtick, `?`, control characters and comments removed from lines |
| `typed_lower_case_g20_switches_units_then_jog` | A `g20` sent with `/sendGCode` goes out as `G20` and switches WebControl to inches (home, jog step); the mm Z panel then moves in inches |
| `lower_case_file_g91_g90_m0_then_resume` | A lower-case file goes out upper-cased: `g91` stops the home shift, `g90` resumes it, `m0` pauses, Resume returns |
| `z_limits_refused_on_51_27` | Z-limit actions alert and send nothing below 51.292 |
| `z_limits_on_51_292` | `$63`/`$64` in mm, clear, `B20`, with 51.292's replies to each |
| `z_lower_limit_pauses_the_job` | 51.292 with a lower limit saved: a file line that would go below it is never sent; the job pauses before it with an alert (Rust-intended deviation 18) |
| `holey_calibration_accept_writes_settings` | Calculate sends nothing; Accept writes `$3`, `$2`, `$40`, `$41` |
| `queue_same_tool_starts_at_once_new_tool_waits` | Tool 1 changed by hand, three queued files: Continue raises Z and runs the first (tool 1); a file with the same tool starts as soon as Z has risen; one with another tool gets `G00 Z…`, `M5`, `T2 M6` and the firmware's prompt first |
| `queue_override_skips_bit_changes` | A queued file with its bit-change override on starts with no `M5`/`T2 M6` though it names tool 2; its own `T2 M6` is never sent and `T3 M6 G0 X60` goes out as `G0 X60` (Rust-intended deviation 16) |
| `park_after_a_file_finishes` | With a park position set, a file shifted by home runs to its end; once its last line is answered, units, `G90`, Z to the safe height and a rapid to the park position, unshifted (Rust-intended deviation 17) |
| `park_at_a_tool_change_then_resume` | Park for Tool Changes on: at a file's `T2 M6` prompt, `~`, then units, `G90`, Z to the safe height and a rapid to the park position with the prompt still up; Resume returns to the file's XY at safe height and streams on, and the file's end parks again (Rust-intended deviation 17) |
| `park_at_a_tool_change_skipped_with_lines_buffered_behind_it` | Park for Tool Changes on, buffered: a tool change WebControl did not expect reaches the firmware with cutting lines buffered behind it; no `~`, nothing written, an alert that the sled did not park (Rust-intended deviation 17) |
| `real_*` | the owner's real jobs, with the controller's recorded replies ("Real jobs" below) |

## Adding or changing a transcript

Transcripts are edited by hand. They are the spec, not a record of what the code does, so the order is: decide the
bytes, write them down, then make Rust produce them.

1. Read the behaviour against `spec/firmware.md` first. Every `rx` must be something the controller really says:
   copy replies from an existing transcript of the same firmware (the 51.27 ones carry the owner's banner, the echo
   of each G/M/T word before its `ok`, and the status reports "Real replies" lists), never invent new ones.
2. Change the smallest thing: edit the `tx` lines the change explains and leave every other event alone. A new
   behaviour gets a new file named after it (`meta.name` equals the file name), usually started from a copy of the
   closest transcript. Keep the 51.27 echo (`rx`) after each changed `tx`.
3. Keep the layout: one event per line, as the existing files are, so a diff shows one event per changed line. Every
   `tx` with `!` or `~` has `"realtime": true`, and only those.
4. Update the table above (a new file) and, where the change departs from what Python did, "Rust-intended
   deviations" below, in the same commit as the Rust change.
5. `cargo test -p wc-testkit --test transcripts` checks the schema and the serde types;
   `cargo test -p wc-machine --test transcripts` replays the corpus against the machine. A failure names the
   transcript and the first event that differs.

Never regenerate a transcript from what the backend now writes: a replay failure is either a bug in the change or a
transcript edit the change needs, and which one is a decision to make by reading `spec/firmware.md`.

## Versioning

`formatVersion` changes only when the transcript format does. A behaviour change in the backend changes the
transcripts it affects, in the same commit, edited by hand and re-checked.

## The Rust runner

`wc-testkit`'s runner implements the replay rules above against the Rust backend with a fake serial port and a
virtual clock. The format is plain JSON designed for `serde`:

- Events are an internally tagged enum: `#[serde(tag = "type", rename_all = "lowercase")] enum Event { Ui { action:
  Action }, Http { .. }, Rx { line: String }, Tx { bytes: String, #[serde(default)] realtime: bool, timeout:
  Option<f64> }, Tick { seconds: f64 }, Expect(Expect) }`, each with `#[serde(default)] comment: Option<String>`.
  `Action.arg`/`arg1` are strings or numbers (`serde_json::Value`, or an untagged enum).
- `setup.gcode` is an untagged enum of `{lines}` or `{path}`. Unknown fields are not allowed anywhere
  (`deny_unknown_fields` is safe), so a typo fails loudly.
- Every number fits `f64`. `tx.bytes` is printable ASCII ending in `\n`; `rx.line` is a string without `\r` or `\n`
  (in practice ASCII, possibly empty: 51.292 prints empty lines). JSON can't carry arbitrary bytes, so a non-UTF-8
  line can't be expressed.
- `setup.settings` values are strings, numbers or booleans as `webcontrol.json` stores them (`Value`);
  `expect.settings[].value` is always a string.
- Numbers inside `tx.bytes` are pinned as the Python backend formats them (`G00 X254.0`,
  `G0 X22.8456 Y3.00`), not as Rust's `{}` would; see "Known warts" 5. A `$n=` value is the exception: it is the
  shortest text the firmware's `readFloat` reads as the float32 nearest the stored value (`$13=63.5`, not Python's
  `63.5000`, which reads as 63.5000038; `spec/firmware.md` §4).

The runner needs these seams in the backend:

1. A serial port trait the runner can implement: the backend writes byte strings and reads lines. Join the writes
   and cut after each `\n`, then compare each line with the next `tx`; how the backend splits its writes is free. A `realtime` tx must still not be queued behind buffered lines.
2. A clock trait (or `tokio::time::pause()` with `advance`) so settles and ticks cost no real time. Advance in steps of
   at most 10 ms so the backend's own timers (write pacing, the 0.25 s resync, the 5 s link timeout) see time pass
   smoothly.
3. A way to know the backend has consumed every rx line (an idle signal, or "the read channel is empty and the
   backend task is parked"), for the settle rule.
4. Settings, units, home and the gcode file set before start, the same way the app loads them; the `ui` events go
   through the same handler the WebSocket `action` frame reaches, and `http` events through the router.
5. `expect.state` names map onto `GET /api/state` and `statusMessage` fields; `emitted` checks the `message` pushes
   the WebSocket transport would send (decoded `data`).

`expect.state` may also name `probing`, which only the probe transcripts use.

The job queue's `http` events take library paths relative to this directory (`gcode/x.nc`), and the runner answers
each load the queue asks for (`Effect::Queue`) at once, as the server's `/openGCode` path would.

The Rust runner replays the sans-IO machine (`Machine::handle_inline`, which also runs the CPU work the shell would
run off the machine task), so it pins the bytes and their order, not how the shell writes them. The shell writes from
a task of its own: a realtime `!` or `~` goes before every line still queued there, and a `!` also drops those lines
and cuts short a line half-written, which the firmware flushes on `!` anyway (`spec/firmware.md` §1). Python's serial
thread also sent its quick queue first and cleared the gcode queue on Stop, but never cut a line short. No transcript
changes: the machine writes nothing between a `!` and the controller's resync.

Rust does not need to reproduce the prelude's tx: it only has to reach the same state from the same rx lines. It
must reproduce every byte after it. Where this contract pins something `spec/firmware.md` says is wrong (below), the
Rust backend may fix it only together with an edited transcript and a note here.

## Recordings from a real machine

With **WebControl Settings → Record serial traffic** on, the Python backend appended every serial line to
`~/.WebControl/serial-recordings/<session>.jsonl`, a new session per connect. The Rust backend does not record (it
logs every serial line to `~/.WebControl/webcontrol-rs.log`); this section is the format of the recordings the
corpus came from. Each line is one JSON object
(`recording.schema.json`) with `t`, monotonic seconds since the session started, and `type`:

| `type`    | Fields                                                     |
| --------- | ---------------------------------------------------------- |
| `session` | `started` (ISO wall time), `webcontrol`, `port`, `units`, `home`, `settings` (Maslow and Advanced sections, key → value), `gcode` (the file loaded at connect, if any); `source` for an imported log |
| `tx`      | `data` (the exact bytes written), `cause` (the `seq` of the most recent UI action, if any) |
| `rx`      | `data` (the line as read, with its `\r\n`)                 |
| `ui`      | `seq`, `command`, `arg`, `arg1`                            |
| `gcode`   | `filename`, `lines` (a file was loaded)                    |
| `dropped` | `count` (lines lost because the writer fell behind), just before the first record kept after them |

Nothing is recorded before the first connect. Writing happens on a native OS thread (not a gevent greenlet, so a slow
disk never blocks the serial loop) from a bounded queue: a full queue drops lines and counts them, so recording never
slows streaming. After a write error the recorder reports it once on the console and records nothing until the next
connect. Files rotate at 5 MB (`<session>.jsonl`, `<session>.1.jsonl`, ...) and the directory keeps the newest 40
files, always including the current session's first file, which holds its `session` line.

`tools/convert_recording.py <recording.jsonl>... [--gcode file.nc] [--name name]` (Python, in git history) wrote a
transcript skeleton to `transcripts/` (and copied the G-code into `gcode/`):

- setup from the `session` line: start `boot` (a session begins at the connect), its units and home, and only the
  settings that differ from `defaultwebcontrol.json`;
- `ui`, `rx` and `tx` events in order, with `tick` for every gap of 0.25 s or more (capped at 4.5 s, under the 5 s
  link timeout);
- a `TODO` comment on the first write of any burst that follows a second or more without writes and no recorded action
  (every such burst in a v0.94 log), and a comment where the recorder dropped records;
- the firmware from the `B05` reply (and 51.292 from its rewrite notice).

Status reports are kept (they end resyncs) unless `--drop-status`. Expect events are not invented; add them by hand,
trim, then replay.

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

- `log.txt` has everything; lines starting `<` or `[` (status, `[PE:...]`) are only there, and it stops while
  logging is suspended (`loggingTimeout`). `alog.txt` has every other line, suspended or not, in the same order.
- Writes from the G-code queue and file end with the backend's trailing space(s); realtime `!` and `~` have none. The
  newline written is not in the log (it always is one `\n`). `Send FAILED: ` marks a write that raised.
- WebControl's own `message_queue` puts are logged like controller lines: calibration input errors (`Message: Please
  enter a number for ...`, `Message: The measurement between ... seems wrong.` followed by more lines with no time) and
  `Message: Cannot open import file.`
- Lines are buffered and appended in batches by a new thread each time, without a lock, so a crash loses the last few
  (and can leave a cut-off line or NUL bytes the next start's header is glued onto), batches can land out of order,
  and time can step back. The clock is wall time, which a Pi sets from NTP after boot.

`tools/import_v094_log.py log.txt [--alog alog.txt] --out DIR` (Python, in git history) splits the log at every
`+++++` header and at every connect, and writes one recording per connect, named by its start time, for `convert_recording.py`. It reads both
files a line at a time and writes as it goes, so an 8 GB log needs no more memory than a small one:

- v0.94 sends `B05`, the units and `$$` when it opens the port, so a connect starts at each `Sent: B05`, or at the
  boot banner (`PCB v1.`, `Grbl v1.00`, `ready`) up to 10 s before it. Lines before a process's first connect are
  dropped.
- `Sent: X` becomes a tx of `X\n`; any other entry an rx of the line plus `\r\n`. Only `\n` ends a line; other
  control characters stay in the data. `Ground Control Version`, the suspend/resume markers and the WebControl
  messages above are dropped; a `Send FAILED:` write is left out with a warning.
- A header glued onto a cut-off line still starts a new process; the cut-off line, and a cut-off last line, are left
  out with a warning. Where time steps back, the rest of the process is shifted forward by the step, with a warning,
  so `t` never decreases.
- The session line gets the units from the first `G20`/`G21` written, and the settings WebControl ended the handshake
  (its first 10 s) with: each `$n=` value from the `$$` reply, replaced by any `$n=` WebControl wrote, mapped back to
  setting keys through `defaultwebcontrol.json`'s `firmwareKey`. Home and the G-code file are not in the log.
- A connect is split into parts (`, part 2` in `source`, same units and settings) at the first write after
  `--split-idle` seconds without one (default 1800) and after `--max-records` records (default 500000). A later part
  starts mid-connection.
- With `--alog`, the two files are walked in step, start by start: entries only `alog.txt` has (written while logging
  was suspended, including the one that triggered the suspend) go in before the next `log.txt` entry, and the entry
  that ended the suspend, which both files have, is kept once.

UI actions are not in v0.94 logs, so the transcript needs its `ui` events added by hand where the TODO comments are.
v0.94's handshake (`B05` first) also differs from today's, so a converted session is a starting point: keep the part
that shows the behaviour (usually a job), restart it from `start: connected`, and hand-fix the tx.

## Real replies

The owner's v0.94 logs (December 2025 to November 2026; holey 51.27 on a PCB v1.1 board, buffering off) were
imported into 366 recordings (13 GB, 1.78 M lines written, 158 M read) and indexed with `tools/index_recordings.py`.
They stay on the owner's machine; only the lines the transcripts carry (gathered in `tools/owner_machine.py`, now in
git history) and the trimmed G-code under `gcode/real/` are in the repository. What they hold, by recording file:

| Feature | Files | Count |
| --- | --- | --- |
| Boot banner / `B05` reply / `$$` report | 33 / 31 / 31 | 100 lines / 31 / 31 |
| Jobs streamed (`G1` with X or Y written) | 40 | 1.63 M lines, 133 k arcs |
| `Tool Change:` / `Maslow Paused` | 25 / 25 | 34 / 35 |
| `~` / `!` written | 26 / 13 | 65 / 24 |
| Jogs (`G91`) / `G10 Z0` | 36 / 26 | 393 / 41 |
| `Command G17 unsupported and ignored.` | 34 | 109 |
| `ALARM:`, `error:`, `Message:`, `Buffer overflow!`, `z axis zeroed`, `G38.2`, B13-B15, `M0` | 0 | 0 |

What 51.27 says, as recorded:

- **Boot:** `PCB v1.1 Detected`, `Grbl v1.00`, `ready`, `ok`, then on its own `[Forward Calculating Position]`,
  `position loaded at:` and the X and Y it loaded, one per line. No `FAKE_SERVO` line.
- **Every G, M or T line is echoed** before it runs, one line per G/M/T word with its arguments (`G0 G40 G90 G17` →
  `G0 `, `G40 `, `G90 `, `G17`), without the trailing spaces; `B05` too; `$` commands, `~` and `!` are not. A `T`
  word adds `Tool change to tool N`; `G17` adds `Command G17 unsupported and ignored.`. The `ok` follows when the
  line finishes, so the echo of a long move comes at once and its `ok` seconds later.
- **`$$`:** every key from `$0` (two decimals) to `$46` (eight), with its description, an empty line, then `ok`. A
  `$45=` write is answered with the position reload lines and `ok`.
- **Status:** `<Idle,MPos:…>` and `[PE:lErr,rErr,space,lChain,rChain]` together about every 0.2 s, idle or cutting;
  `space` is 127 except just after a line arrives.
- **Tool change:** `T2 `, `Tool change to tool 2`, `M6`, `Tool Change: Please insert tool 2`, `Maslow Paused`; a
  `~` then gets two `ok`s (its own and the M6 line's). An M6 for the tool already in gets `T1 `, `Tool change to tool
  1`, `M6`, `ok` and no pause.
- **`!`:** `ok` (two when the line in flight had just finished), then the position reload lines.
- Once (20251221-000533, t=36-53) a lone `M3` got no echo and no `ok`: nothing, not even a status report, came
  for 16 s, then a cut-off `[PE:` line and the boot banner. With `$17=0` (every `$$` in the logs) 51.27's `M3` does
  nothing, so the `M3` itself does not explain the reboot.

## Real jobs

The `real_*` transcripts stream jobs the owner really cut, chosen from the recordings: the trimmed G-code is in
`gcode/real/` (the first lines of each file and the region around its tool change, pause or stop, back to back, never
a whole file), and `real-jobs/<name>.json` holds what v0.94 wrote for those lines and what the controller answered,
status reports thinned. **Both come from the owner's machine**: its files, its home and its controller. Setup uses the
owner's settings (`tools/owner_machine.py`) and the home v0.94 streamed with. The controller answers each line with
51.27's echo, then the status reports the same line got on the machine (ticks of at most 1 s between them), then `ok`.

| Transcript | Job |
| --- | --- |
| `real_mancala_outside_tool_change` | a 137-line contour with arcs, its first pass and last: tool change, Resume, M30 |
| `real_mancala_roughing_tool_change` | the first 150 lines of a 45,000-line adaptive roughing |
| `real_mancala_finishing_large_job` | a 120,000-line finishing job: its first 150 lines and last 40 |
| `real_katie_neck_back_pause_jog_resume` | the logs' only Pause: Z jogged, Define Z zero, Z up, Resume |
| `real_katie_body_top_pause_stop` | tool change answered with Play, helical arcs, Pause in a retract, Stop |
| `real_maypole_tool_change_play` | tool change answered with Play, then the first 150 lines |

`real_katie_neck_back_pause_jog_resume` raises every Z in today's file by 25 mm: the owner regenerated
`1-neck-back-25-endmill.nc` after cutting it, and the logs show the Z values it was cut with.

Every `real_*` transcript has differed from Python since "Never lower before travel" (Rust-intended deviation 2): their
resumes travel at the bit's height, which Python never did, and `real_mancala_roughing_tool_change`,
`real_mancala_finishing_large_job` and `real_katie_body_top_pause_stop` wait for the status report the controller
sent after the `~` (copied from their recordings, its `[PE:]` line after the first write it releases). They are
hand-edited.

`tools/extract_real_job.py` cut both files from the recordings (its `JOBS` table named each job's recording, time
windows and line ranges), `tools/real_job_scenarios.py` held the scenarios, and `tools/compare_real_jobs.py` wrote
`real-jobs.md`: every write the backend makes for these lines against what v0.94 made, each difference either an
intended change with its finding or commit, or UNEXPLAINED. All three are Python, in git history. A behaviour change
that touches a real job now edits its transcript by hand and updates `real-jobs.md` to match.

## Known warts

Behaviour the transcripts pin today that `spec/firmware.md` or plain sense says is off. A Rust backend matches it
until a transcript and this list change together.

1. **Realtime commands carry a newline.** `!` and `~` go out as `!\n` and `~\n`. This one is harmless: the firmware
   acts on the character at once and drops a `\n` that follows a realtime character (`quickCommandFlag`,
   `GCode.cpp` in both 51.27 and 51.292), so nothing reaches its line buffer.
2. **Resume always sends `~`**, even after a plain Pause, when the firmware is not paused. It costs an extra `ok`.
3. **Doubled spaces.** Most lines end in two spaces (`G21  `, `G0 Z5  `); `home` and `returnToCenter` send `G90   `.
4. **Handshake order.** Python sent `G21`, `$$`, `G90` before `B05` and the `$13` write, so `$$` was compared
   before the firmware version was known. Rust sends `B05` first, then the units, `G90` and `$$`, and then, as
   Python did, writes only `$13` and the settings `$$` shows differ (`spec/firmware.md` B2). Rust once wrote every
   setting after every boot instead; that wrote `$38`, which re-runs `setupAxes()` on the live board, and the owner's
   motors stopped following (2026-11-15). The `connect_*` transcripts pin the handshake (the prelude's tx is not
   checked, and its `$$` gets a bare `ok`, so a prelude writes only `$13`).
5. **Number formats vary.** A restore writes `$16=0.000000`, `$21=1300.00`, `$45=0.00000516850`: whatever each
   setting's formatter produces. Moves the backend builds print Python's `str(float)` (`G00 X254.0`, `G0 Z5.0`), and
   home-shifted file coordinates keep the longer operand's decimals (`X22.8456 Y3.00`).
6. **Controller replies are real only where the owner's recordings show them.** The 51.27 transcripts answer as the
   owner's controller (holey 51.27, PCB v1.1) did in 13 GB of v0.94 logs (`tools/owner_machine.py`, "Real replies"
   below): its banner (`PCB v1.1 Detected`, `Grbl v1.00`, `ready`; no `FAKE_SERVO` line), the echo of every
   G/M/T word before a line runs, `Tool change to tool N`, `Command G17 unsupported and ignored.`, the position
   reload after a boot and after `!`, the full `$$` report in `connect_boot_handshake`, and recorded status streams in
   the `*_waits_for_a_slow_ok` pair. Still simplified: the prelude (the runner's own: `$$` gets a bare `ok`, `B05`
   no echo); status reports a scenario places at chosen positions, which have no `[PE:]` line; the coordinates in
   `position loaded at:` (the last status report's, or 0); and every reply the owner's machine never produced in the
   logs: `M0`'s pause, ALARM, `error:`, `Buffer overflow!`, the probe's lines, `Message:`, the `$` write replies
   and all of 51.292. The default `bootBanner` (TLE9201 with `FAKE_SERVO off`) is not what 51.27 prints; it stays
   the runners' default, and the 51.27 transcripts set the owner's banner explicitly. A Rust parser must also
   tolerate the full replies in `spec/firmware.md` §1, §9.
7. **The line limit counts bytes in Rust (Rust-intended).** A line is refused when, as written with its `\n`, it is
   over 126. Python counts characters; Rust counts bytes, as the firmware's buffer does. The two agree on every
   transcript, which is ASCII; they differ only for non-ASCII text, which Rust refuses sooner.
8. **Line pacing is v0.94's.** A line due under 50 ms after the previous write waits a full 50 ms from when it became
   due (`MINTimePerLine`: `_write` slept 50 ms, not the rest of it), so lines due back to back go about 60 ms apart.
   `!` and `~` are never paced. Rust once waited only for the rest of the 50 ms and, on the owner's cuts, lost
   about 1 line in 290 (each written 0–40 ms after the previous `ok`), where v0.94 lost none in 165,876; it returned
   to v0.94's pacing on 2026-11-15 (`spec/firmware.md` B3). The runner checks only that a line comes after what
   releases it and within `txTimeout`, so no transcript changed; `wc-machine`'s `tests/link.rs` pins the timing.
   Since 2026-11-16 the pacing is two settings, `linePacingMode` (`v0.94`, the default, or `minimum gap`: wait only
   for the rest of the gap since the last write) and `linePacingGapMs` (0–500, default 50), changeable mid-job from
   the run panel (`PUT /api/run/pacing`) and in effect from the next line. Transcripts run with the defaults, so
   every one replays as before; `wc-machine`'s `tests/pacing.rs` pins each mode at 0, 25, 50 and 100 ms. Each
   change, job start and job end writes a `PACING` line to the serial log (B3).

## Rust-intended deviations

Where the Rust backend writes something different from Python by owner decision.

1. **Triangular Accept raises Z first** (decision 3, 2026-11-15). Python's Accept queues `G21 `, `G90 `, `G40 `,
   `G0 X0 Y0 ` after the `$` lines, so the sled travels to the centre at whatever height the bit is. Rust queues
   `G0 Z<zAxisSafeHeight in mm> ` before `G0 X0 Y0 ` (after `G21`, so in millimetres), and refuses the Accept with a
   409 or an alert, saving nothing, when the safe height is not a number above 0. No transcript pins it:
   `fixtures/calibration/accept_commands.json` stays Python's record, and `wc-calibration`'s fixture test inserts the
   raise.
2. **Never lower before travel** (2026-11-15). Before the XY travel of every resume and restart (Resume after Pause, an
   M0 or a tool change; Play mid-file; Play after an ALARM or overflow) and of Home, Return to Center and triangular
   Accept, Python sent `G0 Z<zAxisSafeHeight>` (`G00 Z…` for Home and Return to Center) even when the bit was higher,
   so a bit 10 mm up was lowered to 5 mm and then travelled. Rust sends the same one line, at max(safe height, the
   bit's Z) in the active units, and no Z move before the travel lowers the bit. Keeping the line when the bit is
   already higher (a move to where it is) keeps every sequence the same shape, so a transcript changes only in that
   line's value. The bit's Z is the Z WebControl last commanded, followed through every line it wrote (relative
   moves, units, `G10 Z0`, a touching probe's zero); when a Stop, an ALARM, a missed probe, a line it cannot follow
   or a reboot leaves that unknown, it is the last status report's MPos z plus one display step (0.01 in the
   report's units), since MPos has two decimals and is only a lower bound here (`spec/firmware.md` B10, B15). A report
   counts only once the units are known from a `G20`/`G21` WebControl wrote (the handshake's) and while no line that
   moves Z or changes units is unacknowledged, since the firmware reports mid-move and acks a line when it finishes.
   The height is settled when the line reaches the controller, after every line queued before it (a Z jog pressed just
   before Resume counts). With neither known then (no Z followed and no report since the last line that moved Z: after
   a Stop, an ALARM or overflow, a missed probe, a reconnect or a reboot), nothing is guessed: the raise, and every
   command queued after it, waits for a status report read after that point (one comes about every 200 ms,
   `spec/firmware.md` §2) and uses its MPos z rounded up. With no report for 3 s it is refused instead: nothing
   queued after it goes out, a `Bit Height Unknown` alert says so, a resume is left paused (Resume tries again) and
   Play mid-file leaves the job stopped at its line. Stop drops the waiting raise at once, as any queued command;
   the wait holds only the queue, never the machine task. Python sent the safe height at once, so a transcript whose
   height is unknown there reads a status report before the raise: `setup.sled` for those that start with none
   (Python replayed them unchanged), one after a missed probe, and the recorded one after the `~` for the real jobs.
   Transcripts: `resume_above_safe_height_never_lowers_before_travel`; `real_katie_neck_back_pause_jog_resume`,
   `real_mancala_outside_tool_change` and `real_maypole_tool_change_play`, whose resumes now travel at the bit's
   10 mm (or the reported 10.03 mm, sent as `Z10.04`) instead of 5 mm; `real_mancala_finishing_large_job` and
   `real_katie_body_top_pause_stop`, which wait for the report after the `~` and travel at `Z10.04` and `Z15.05`;
   `real_mancala_roughing_tool_change`, which waits for it and travels at 5 mm above the bit's -0.02 mm; and
   `probe_in_file_misses_and_waits_at_the_probe`, which waits for the report after a missed probe.
3. **The spindle's speed after `M3` on 51.29x** (2026-11-15). 51.29x leaves the PWM at 0 after `M3` until an `S` line
   (`spec/firmware.md` §9, B13), so a resume or restart that restores `M3` or `M4` on a controller reporting 51.29
   follows it with the file's last `S` on its own line (`S12000.0`), if the file set one. On 51.27 and stock the
   restart sends the bare `M3`, as before. Transcript: `resume_on_51_292_sets_the_spindle_speed_after_m3`.
4. **An alarm holds everything that moves** (2026-11-15). Python's resync after an ALARM or `Buffer overflow!`
   (35363b3) released the commands still queued behind the halted line once the status report came, so on the owner's
   machine Home's `G00 Z0` went out 0.4 s after the firmware had flushed the travel (v0.94 never acked the halted line
   and so held its queue until Stop). Rust drops those commands and refuses new ones until Stop, or Play on the held
   job (`spec/firmware.md` B17). Transcript: `alarm_mid_home_holds_the_plunge_until_stop`, built from the owner's
   `webcontrol-rs.log` lines.
5. **A `$38` write resets the controller** (2026-11-15). Python wrote `$38` like any setting and left the board
   running with re-initialised axes. Rust writes `$38` only when `$$` shows it differs (or after an EEPROM reset,
   in the upgrade's migration, or for a settings change), and once the `$` lines with it are answered it closes and
   reopens the port, which resets the Mega through DTR; the lines queued behind them are dropped with the old link.
   Every action that could move the machine is refused until the next boot banner; a reopen with no banner gets an
   alert to power-cycle the controller. After that boot `$$` shows `$38` held, so it is not written again; if it
   still differs it is skipped with an alert, so the reset cannot loop. `$38` is never written while a job holds the
   machine. The transcript runner reopens the port as the shell does; the banner after a reset is an `rx`.
   Transcripts: `connect_chain_over_sprocket_differs_restarts_the_controller`,
   `connect_eeprom_read_fail_restores_settings`.
6. **A line the controller never received goes out again** (2026-11-15). On the owner's cut a written arc got no
   echo and no `ok`, and every report showed `[PE:]` space 127 and the same MPos until Stop, 44 s later; Python
   would have waited the same way. Rust takes the unacknowledged lines as lost when, since the last ack, the
   controller printed only status reports and three in a row, each read at least 150 ms after the latest write,
   showed an empty ring, an unchanged MPos and `Idle` (`spec/firmware.md` B18): the firmware reads the port into
   its ring before every report, so a line that arrived would show in the ring or be echoed first; on the owner's two
   cuts every line received was echoed within 40 ms. Rust then writes the same bytes again once, with a `controllerMessage` naming the line. Lost
   again, a job line pauses the job at it with a `Line Not Received` alert, and Resume returns to the file there as
   after any pause; a lost command is dropped with what was queued after it. Only a line written to an idle
   controller is checked, since an `ok` behind a lost buffered line is counted for the lost one. A job paused by the user waits at the
   lost line for Resume. Transcripts: `stream_lost_line_is_sent_again`, built from the owner's
   `webcontrol-rs.log` lines, and `stream_line_lost_twice_pauses_until_resume`.
7. **Lines are read and sent upper-cased** (2026-11-15). The firmware upper-cases every line before it parses it
   (`spec/firmware.md` §1), but Python checked lines case-sensitively and wrote them as the file or the user had
   them (only absolute file lines were upper-cased, by the home shift). So a typed `g20` put the controller in inches
   while WebControl stayed in mm, a lower-case `m0` or `m6` streamed on without a pause, `g91` and `g90` were missed,
   and a restart dropped a lower-case `m3`. Rust upper-cases each line once its comments are stripped, before it
   checks or writes it, and reads the file upper-cased for the restart state, the positions and the preview (the
   loaded text keeps the case it was written in). No upper-case line changes, so no earlier transcript does.
   Transcripts: `typed_lower_case_g20_switches_units_then_jog`, `lower_case_file_g91_g90_m0_then_resume`.
8. **No restart from an unknown XY** (2026-11-15). Python's restart state started X and Y at home, so a file that
   moves X or Y relatively (G91) before it sets them absolutely was restarted, or resumed after a pause, by
   travelling to home plus the file's relative moves, wherever the file had really started; every relative move
   after that cut in the wrong place. Rust refuses Play mid-file and Resume at any line before such a file sets X
   and Y absolutely, even one before its first X or Y move, with an alert, sends nothing (no `~`; a paused job stays
   paused) and keeps the index; Stop and Play from the first line still run the file. A file that sets X and Y absolutely before its relative passes restarts as before. No transcript has such a
   file (`wc-machine` tests `resume_and_play_mid_file_are_refused_before_a_g91_file_sets_x_and_y`).
9. **Arcs go out as the file means them** (upstream #120, 2026-11-15). Python streamed G2/G3 untouched. 51.27 has
   no R word, so an R arc sent the sled straight to its end at PID speed; 51.29x reads R but drops its sign; and
   every version runs an arc's X, Y and Z as absolute under G91 (`spec/firmware.md` §9, A18, A19, B19). On every version
   Rust rewrites, as the line is written and from where it last commanded the sled (or the last status report),
   each R arc (no I or J) with I and J at 4 decimals and each arc under a real G91 with the absolute X and Y it ends
   at. An arc it cannot rewrite (radius short of half the chord, an R arc with no X/Y change or under G18/G19, a
   G91 arc with Z, no known start) is refused as an unclosed comment is (B19). Transcripts:
   `r_arc_converted_to_i_j_on_51_27`, `r_arc_impossible_radius_pauses_the_job`, `r_arc_converted_on_51_292`,
   `g91_arc_written_absolute`, `g91_arc_with_z_pauses_the_job`.
10. **No raise to a safe height of 0 or less** (2026-11-17). Python raised to whatever `zAxisSafeHeight` held, so at
    0 Home sent `G00 Z0.0` and travelled with the bit at the work surface, and below 0 plunged before travelling.
    Rust refuses Home, Return to Center, Play mid-file, every Resume that returns to the file, triangular Accept and
    an optical scan with an alert (or a 409) naming the setting and the value it holds, and sends nothing (no `~`; a
    paused job stays paused). The settings form refuses to save such a value (`spec/firmware.md` B10). Transcript:
    `home_refused_at_a_safe_height_of_zero`.
11. **G38.3, G38.4 and G38.5 are never sent** (2026-11-17). 51.27 and 51.292 probe only on G38.2; any other G38 word
    prints that it is invalid and acks `ok` with no move and no `z axis zeroed` (`spec/firmware.md` §4), so a file
    that relies on it cuts from a Z that was never set. Python sent them. Rust refuses them as it refuses an unclosed
    comment: a typed line is dropped with a `Command Unsupported Probe` alert, and a job pauses before the line with
    a `Line Unsupported Probe` alert (B16). Transcript: `probe_g38_3_4_5_refused_typed_and_in_file`.
12. **G90.1 and G91.1 never reach the controller** (2026-11-17). Every firmware truncates the G number to an int,
    so Python's untouched `G91.1`, which VCarve posts write near the top of every file, switched every later G0/G1
    to relative, and `G90.1` arcs were cut about the wrong centre, since the firmware always reads I and J as
    offsets (`spec/firmware.md` §9, A20, B19). Rust leaves each G90.1 or G91.1 segment out of every line it writes
    (a line left blank is not written; a job counts it as sent), keeps the arc-centre mode itself, and writes each
    G90.1 arc's centre, shifted to home with its X and Y, as I and J offsets from its start. A restart's queued
    lines end with the file's mode there (`G90.1 ` or `G91.1 `), which is taken and never written. An arc rewritten
    from a start only a status report gives is refused (`Arc Start Uncertain`) when the report's two decimals could
    put it more than 0.05 mm from where the file means, as an exact half circle can be. Transcripts:
    `g91_1_left_out_of_a_vcarve_file`, `g90_1_arc_centres_written_as_offsets`.
13. **A byte order mark is dropped at load** (2026-11-18). Python read files as plain UTF-8, so a file saved with a
    BOM began `\u{feff}G21`; the restart state never saw that G21, and Play mid-file on a machine set to inches sent
    `G20` and ran a millimetre file in inches. Rust drops one leading U+FEFF before it filters the file, so what is
    streamed, drawn and restarted from all start at the first word. No transcript has such a file (`wc-machine`
    test `a_byte_order_mark_does_not_hide_the_files_units_from_a_restart`).
14. **No arc outside G17 or with no centre** (2026-11-18). Every firmware ignores G17/G18/G19 and cuts each arc in XY
    (`spec/firmware.md` §9), and an arc with no I, J or R has radius 0, so the sled goes straight to its end at PID
    speed; Python sent both. Rust refuses, as in 9, any G2/G3 (or modal arc) with an axis or centre word while
    G18 or G19 is in effect (`Unsupported Arc`), not only an R arc, and any arc with an X, Y or Z word but no I, J
    or R (`Impossible Arc`). An arc segment with neither, such as a bare `F500`, still goes out. The plane is the
    file's, since the firmware keeps none: a job run from its first line starts in G17, the G17, G18 and G19 it
    streams change it, and a restart takes it from the lines before the restart point, as it does the units. A typed
    G17, G18 or G19 sets it for the typed arcs after it, until the next job starts. No transcript has such an arc
    (`wc-machine` tests `a_job_pauses_before_an_arc_in_the_g18_plane`,
    `play_mid_file_in_a_g18_section_refuses_its_arc_though_g18_never_went_out`,
    `a_g18_from_a_typed_line_or_an_earlier_run_is_forgotten_by_the_next_run_and_by_a_restart_after_g17`,
    `a_typed_arc_with_no_centre_is_refused_with_an_alert`).
15. **A restart inside a run of modal arcs keeps cutting arcs** (2026-11-18). A restart's moves leave the firmware
    in G0 or G1, and Python restored only G0/G1, so a bare `X20 Y0 I5 J0` after `G2 ... ` was cut as a straight
    G1 line after Play mid-file or Resume. Rust writes the file's `G2 ` or `G3 ` before the first file line after
    the restart that has an X, Y, Z, I, J or R word and no G word, then converts it as any arc; a line with G0 to G3
    of its own needs none, and lines with neither (comments, `F`, `M`, `S`) pass untouched until one comes. No
    transcript restarts mid-arc (`wc-machine` tests `play_mid_file_inside_a_run_of_modal_arcs_writes_the_files_g2`,
    `resume_inside_a_run_of_modal_arcs_writes_the_files_g3`).
16. **A queued file's override skips its bit changes** (owner decision, 2026-11-22). Python had no job queue. In a
    queue run of a file added with "Don't stop for bit changes", every line that would pause for a tool change (an
    M6 whose tool differs from the one the firmware holds) goes out without its T and M6 words, keeping the rest of
    the line, and a line left with nothing is not sent; the firmware pauses on M6 by itself (`spec/firmware.md` §7),
    so leaving the words in would still stop it. No `M5` or `T<n> M6` goes out between files for it either. Each
    skipped change is logged (`QUEUE BIT CHANGE SKIPPED file_line=<n> tool=<t>`). The firmware keeps the tool it had;
    the queue follows the tools the file names, so the next file is compared with the file's last tool. Play of the
    same file outside the queue pauses as before. Transcript: `queue_override_skips_bit_changes`.
17. **The sled parks after a job** (owner decision, 2026-11-22). Python had no park position. With `parkX` and `parkY`
    set (Maslow Settings, mm, machine coordinates), once a file run from Play, or the queue's last file, has had its
    last line answered, Rust writes the units WebControl is in (`G21 ` or `G20 `), `G90 `, raises Z as Home does (to
    max(safe height, the bit's Z), never lower) and rapids to the park position converted to those units
    (`G00 X-300.0 Y200.5 `). The controller keeps no work offset, since WebControl adds home to each file line
    itself, so the park move is not shifted and Set Home never moves it. Z stays up. It parks whenever the file reaches its end,
    resumed or not. Nothing parks between queued files, or when Stop, an alarm, a lost link, a controller reset, a
    refused or twice-lost line or a failed probe ends the job before its end, or for a queue Pause stopped; Pause, a
    command sent before the last line is answered, or an `error:` for it, cancels it. The Park action sends the same lines when idle. A park the gates refuse (safe height 0 or less, off
    the bed, an alarm, a missed restart) writes nothing, logs `PARK SKIPPED` and alerts. Transcript:
    `park_after_a_file_finishes`. With Park for Tool Changes on (`parkForToolChange`, off by default), a job's tool
    change, once the firmware has paused for it, gets `~` (as a jog does) and the same park lines, the prompt still up;
    Resume returns to the line at safe height as after a jog. The queue's bit change between files parks too; Resume
    first raises Z and rapids back to where the sled was. Touch Z while parked, for a job's tool change or the queue's,
    raises Z and rapids back over the work instead of probing at the park position; pressed again, it probes. An
    alarm, a missed restart, no park position, a line Resume could not return to, or anything but the bare M6 waiting
    in the controller or WebControl's queue (job lines buffered behind it, a queued jog, an M6 line that moves on after
    its M6), all of which the `~` would run before the park, leaves the prompt where it is, with no `~`, an alert and
    `PARK SKIPPED trigger=tool_change`. Transcript: `park_at_a_tool_change_skipped_with_lines_buffered_behind_it`. Transcript: `park_at_a_tool_change_then_resume`.
18. **Z travel limits checked by WebControl** (owner decision, 2026-11-22; upstream #136). Python only set the
    firmware's (`z_limits_on_51_292`), which 51.292 clamps Z to silently. WebControl mirrors `$63`/`$64` from `$$`,
    from `new zaxis upper/lower limit=` and from B20 (in the controller's units); a reboot forgets them until its `$$`.
    Every line it writes that would take Z past one, followed in mm through G20/G21 and G90/G91 from the Z it last
    commanded (or the last status report), is not written: a file line pauses the job before it (`Line Beyond Z
    Limit`), a command is dropped with an alert, and so is everything queued after it, with `G90 ` written if a dropped
    line would have restored it. A Z WebControl cannot follow is left to the firmware. After a G10, a touch or a manual
    Z adjust (which move the limits by an amount only the firmware knows) the mirror is cleared and `B20 ` queued.
    Touch Z probes no deeper than the lower limit. Transcript: `z_lower_limit_pauses_the_job`.
