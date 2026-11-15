"""Scenarios the recorder turns into contract/serial transcripts: a setup and steps, without the tx lines."""
import owner_machine

STATUS = "<Idle,MPos:{x:.2f},{y:.2f},{z:.2f},WPos:0.000,0.000,0.000>"


def ui(command, arg="", arg1="", comment=None):
    return note({"type": "ui", "action": {"command": command, "arg": arg, "arg1": arg1}}, comment)


def http(path, method="POST", form=None, json=None, status=None, comment=None):
    event = {"type": "http", "method": method, "path": path}
    event.update({"form": form} if form is not None else {"json": json} if json is not None else {})
    return note({**event, **({"status": status} if status is not None else {})}, comment)


def rx(line, comment=None):
    return note({"type": "rx", "line": line}, comment)


def status(x=0.0, y=0.0, z=0.0, state="Idle"):
    return rx(STATUS.format(x=x, y=y, z=z).replace("Idle", state))


def tick(seconds, comment=None):
    return note({"type": "tick", "seconds": seconds}, comment)


def expect(comment=None, **checks):
    return note({"type": "expect", **checks}, comment)


def note(event, comment):
    return {**event, "comment": comment} if comment else event


# Recorder-only steps: they drive the controller model and never appear in a transcript.
def idle():
    return {"type": "idle"}


def wait_tx(pattern):
    return {"type": "waitTx", "pattern": pattern}


def firmware(ack=None, flush=False, rule=None, forget=False, echo=None, reports=None):
    '''
    forget: the model drops what it holds and ignores every line written so far, so the scenario answers them itself.
    echo: the model prints what 51.27 prints before running a line (its echo, tool and unsupported notes).
    reports: called with each line before its ok, for the status reports (and ticks) to send while it runs.
    '''
    return {"type": "firmware", "ack": ack, "flush": flush, "rule": rule, "forget": forget, "echo": echo,
            "reports": reports}


def pauses_on(pattern, *replies):
    return firmware(rule={"pattern": pattern, "replies": list(replies), "hold": True})


SCENARIOS = {}


def scenario(name, description, setup, steps, firmware_meta=None, timing=None):
    '''
    A 51.27 scenario answers as the owner's controller does: its boot banner and its echo of each line.
    '''
    firmware_meta = firmware_meta or {"variant": "holey", "version": "51.27"}
    real = firmware_meta["version"] == "51.27"
    connected = setup.get("start", "connected") == "connected"
    SCENARIOS[name] = {
        "name": name,
        "description": description,
        "firmware": firmware_meta,
        "setup": {**setup, "bootBanner": owner_machine.BOOT} if real and connected else setup,
        "steps": [firmware(echo=True)] + steps if real else steps,
        **({"timing": timing} if timing else {}),
    }


MM = {"units": "MM", "home": {"x": 0, "y": 0}}
INCHES = {"units": "INCHES", "home": {"x": 0, "y": 0}}
BUFFERED = {"Maslow Settings": {"bufferOn": 1}}
BANNER = owner_machine.BOOT_BANNER
SLED_HALTED = ("ALARM: The sled is not keeping up with its expected position and has halted. Click the 'Stop' button "
               "to clear the alarm. More information at: https://github.com/MaslowCNC/Firmware/wiki/Keeping-Up  ")
TOOL_2 = ["Tool Change: Please insert tool 2", "Maslow Paused"]
DIRECTIONS = ["up", "upRight", "right", "downRight", "down", "downLeft", "left", "upLeft"]
FINISHED = {"uploadFlag": 0, "jobInProgress": False, "gcodeIndex": 0}


def gcode(name):
    return {"path": "gcode/" + name}


def resync(x=0.0, y=0.0, z=0.0):
    '''
    After "!", ALARM or an overflow the backend ignores acks until a status report read after 0.25 s of quiet.
    '''
    return [tick(0.3, "the backend ignores acks until a status report after 0.25 s"), status(x, y, z)]


# ----- connect -----

scenario(
    "connect_boot_handshake",
    "The port opens and the controller boots: WebControl sends units, $$ and G90 once the boot ok arrives, then the "
    "B05 version query and the $13 write the banner queued. The B05 reply is parsed. The controller's lines are the "
    "owner's 51.27 on a PCB v1.1 board, its full $$ report included, so nothing differs and nothing more is written.",
    {**MM, "start": "boot", "settings": owner_machine.SETTINGS},
    [firmware(rule={"pattern": r"^\$\$", "replies": owner_machine.SETTINGS_REPORT}), *[rx(line) for line in BANNER],
     rx("ok", "the boot ok releases the queued commands"), *[rx(line) for line in owner_machine.POSITION_LOADED],
     idle(),
     expect(state={"firmwareBuild": "51.27", "controllerBoard": "PCB v1.1 Detected"},
            emitted=[{"command": "controllerMessage", "data": "Firmware Version 51.27"}])],
)

SETTINGS_REPORT = [
    "$0=2438.40000000 (Machine width, mm)", "$1=1219.20000000", "$2=2978.40000000", "$3=478.00000000",
    "$9=2000", "$12=8113.73000000", "$13=63.50000000", "$15=800", "$16=0", "$18=12.60000000", "$19=3.17000000",
    "$28=1.00000000", "$36=1.00000000", "$45=0.00000517 (chain stretch factor, m/m/N)", "$46=97.90000000",
]

scenario(
    "connect_settings_sync_adopts_unknown_keys",
    "The $$ reply is compared with webcontrol.json: $18, $9, $28 and $36, which WebControl has never known, are "
    "adopted without a write; $3, which differs, is rewritten; $45 printed rounded to 8 decimals is left alone.",
    {**MM, "start": "boot"},
    [firmware(rule={"pattern": r"^\$\$", "replies": SETTINGS_REPORT}), *[rx(line) for line in BANNER], rx("ok"),
     *[rx(line) for line in owner_machine.POSITION_LOADED], idle(),
     expect(settings=[{"section": "Advanced Settings", "key": "maxZRPM", "value": "12.6"},
                      {"section": "Advanced Settings", "key": "axisDetachTime", "value": "2000"},
                      {"section": "Advanced Settings", "key": "propWeightV", "value": "1"},
                      {"section": "Advanced Settings", "key": "propWeightVZ", "value": "1"},
                      {"section": "Maslow Settings", "key": "motorOffsetY", "value": "463"}])],
)

scenario(
    "connect_eeprom_read_fail_restores_settings",
    "A boot that reports `error: EEPROM read fail` (every firmware upgrade or downgrade) gets every setting written "
    "back, unknown keys from their defaults, then a $$ to verify, and the user is told.",
    {**MM, "start": "boot"},
    [rx(BANNER[0]), rx("error: EEPROM read fail. Using default settings."), rx(BANNER[1]), rx(BANNER[2]), rx("ok"),
     idle(),
     expect(alerts=[{"title": "Alert", "message": "lost its settings (EEPROM read fail)"}],
            settings=[{"section": "Advanced Settings", "key": "maxZRPM", "value": "12.6"}])],
)

scenario(
    "connect_without_a_reset_handshakes_after_3_s",
    "A board that does not reset when the port opens prints no banner and no boot ok, only its status reports. "
    "After 3 s without either WebControl sends the handshake anyway, with the B05 and $13 a banner would have queued.",
    {**MM, "start": "boot"},
    [status(), tick(1.0, "no banner, no ok"), status(), tick(1.0), status(),
     tick(1.0, "3 s after the port opened the handshake goes out"), idle(),
     expect(state={"firmwareBuild": "51.27"},
            emitted=[{"command": "controllerMessage", "data": "Firmware Version 51.27"}])],
)

# ----- manual moves -----

scenario(
    "jog_each_direction_mm",
    "Each of the eight jog buttons in mm: G91, one rapid, G90, each line waiting for the previous ok.",
    {**MM},
    [step for direction in DIRECTIONS for step in (ui("move", direction, "10"), idle())]
    + [expect(state={"positioningMode": "G90", "uploadFlag": 0})],
)

scenario(
    "jog_each_direction_inches",
    "Each of the eight jog buttons in inches: the distance goes out as typed, with no G20 (the controller is "
    "already in inches from the connect).",
    {**INCHES},
    [step for direction in DIRECTIONS for step in (ui("move", direction, "0.25"), idle())],
)

scenario(
    "home_at_safe_height_mm",
    "Home raises to the safe height, rapids to the home position and lowers to Z0.",
    {"units": "MM", "home": {"x": 100.5, "y": -50.25}},
    [ui("home"), idle()],
)

scenario(
    "return_to_center_inches",
    "Return to Center in inches: the safe height is converted from mm and printed with three decimals.",
    {**INCHES},
    [ui("returnToCenter"), idle()],
)

scenario(
    "define_home_from_sled_then_home",
    "Define Home with no arguments takes the sled position from the last status report and sends nothing; the next "
    "Home goes there. Define Home with arguments takes inches and stores mm on a mm machine.",
    {**MM, "sled": {"x": 12.34, "y": -5.5, "z": 0.0}},
    [ui("defineHome"), expect(state={"home": {"x": 12.34, "y": -5.5}},
                              emitted=[{"command": "homePositionMessage", "data": {"xval": 12.34, "yval": -5.5}}]),
     ui("home"), idle(),
     ui("defineHome", 10, 5), expect(state={"home": {"x": 254.0, "y": 127.0}}),
     ui("home"), idle()],
)

scenario(
    "move_z_raise_lower_and_zero_mm",
    "Z moves in mm (units, G91, the move, G90), then Define Z zero (G10 Z0).",
    {**MM},
    [ui("moveZ", "raise", "5"), idle(), ui("moveZ", "lower", "1.5"), idle(), ui("defineZ0"), idle()],
)

scenario(
    "move_z_in_inches_on_a_mm_machine",
    "The Z panel in inches on a mm machine: each move goes out converted to mm, so neither the controller's units "
    "nor WebControl's units and home change.",
    {"units": "MM", "unitsZ": "INCHES", "home": {"x": 100.3, "y": -50.25}},
    [step for _ in range(3) for step in (ui("moveZ", "raise", "0.25"), idle())]
    + [expect(state={"units": "MM", "home": {"x": 100.3, "y": -50.25}})],
)

# What G38() prints on 51.27 before the probe line's ok or error; the feed is the boot feed clamped to zMaxFeed.
PROBING = ["probing for z axis zero", "max depth -10.00 mm.", "feedrate 39.94 mm per min."]
PROBE_TOUCHED = PROBING + ["z axis zeroed"]
PROBE_MISSED = PROBING + ["error: probe did not connect", "program stopped", "z axis not set", ""]

scenario(
    "touch_z_mm",
    "Touch Z probes with G38.2 down to the plunge limit. On a touch the firmware zeroes Z itself and acks, so "
    "nothing follows the probe line.",
    {**MM},
    [firmware(rule={"pattern": r"G38\.2", "replies": PROBE_TOUCHED}), ui("touchZ"), idle(),
     expect(noAlerts=[{"title": "Touch Z"}])],
)

scenario(
    "touch_z_inches",
    "Touch Z on an inch machine probes in inches: the plunge limit (mm) is converted, so the units never change.",
    {"units": "INCHES", "home": {"x": 3.9488, "y": 0}},
    [firmware(rule={"pattern": r"G38\.2", "replies": PROBE_TOUCHED}), ui("touchZ"), idle(),
     expect(state={"units": "INCHES", "home": {"x": 3.9488, "y": 0}})],
)

scenario(
    "touch_z_probe_fails_leaves_z_alone",
    "A probe that never touches: the firmware's `error: probe did not connect` stands in for the line's ok, so "
    "nothing zeroes Z and the user is told. The next command goes out as usual.",
    {**MM},
    [ui("touchZ"), wait_tx(r"G38\.2"), firmware(forget=True), *[rx(line) for line in PROBE_MISSED], idle(),
     expect(alerts=[{"title": "Touch Z Failed", "message": "Z was not zeroed"}]),
     ui("moveZ", "raise", "5"), idle()],
)


PROBING_REFUSAL = {"title": "Alert", "message": "Wait for Touch Z to finish or press Stop"}


def resume_refused_mid_probe():
    '''
    A paused job, then touchZ, then Resume while the probe descends.
    '''
    return [ui("startRun"), wait_tx("X10.5 Y10.25"), ui("pauseRun"), idle(),
            ui("touchZ"), wait_tx(r"G38\.2"), firmware(forget=True), *[rx(line) for line in PROBING],
            ui("resumeRun", comment="the probe is still descending: refused, so no ~ and no restart"),
            expect(state={"uploadFlag": 2, "jobInProgress": True, "pauseButtonSetting": "Resume"},
                   alerts=[PROBING_REFUSAL])]


scenario(
    "resume_during_probe_is_refused_until_the_probe_touches",
    "Resume pressed while a touchZ probe descends during a paused job is refused with an alert, so no ~ reaches the "
    "firmware and no restart is queued behind the probe. The probe touches and the firmware zeroes Z itself, so "
    "nothing follows its ok; the next Resume returns to the job.",
    {**MM, "gcode": gcode("square_mm.nc")},
    [*resume_refused_mid_probe(), rx("z axis zeroed"), rx("ok", "the probe line's own ok"),
     expect(noAlerts=[{"title": "Touch Z"}]), ui("resumeRun"), idle(), expect(state=FINISHED)],
)

scenario(
    "resume_during_probe_is_refused_then_the_probe_misses",
    "Resume pressed while a touchZ probe descends during a paused job is refused with an alert. The probe misses: "
    "its `error: probe did not connect` raises the Touch Z Failed alert and nothing zeroes Z; the next Resume "
    "returns to the job.",
    {**MM, "gcode": gcode("square_mm.nc")},
    [*resume_refused_mid_probe(), *[rx(line) for line in PROBE_MISSED[len(PROBING):]],
     expect(alerts=[{"title": "Touch Z Failed", "message": "Z was not zeroed"}]), ui("resumeRun"), idle(),
     expect(state=FINISHED)],
)

# ----- streaming -----

SHIFTED_HOME = {"units": "MM", "home": {"x": 12.3456, "y": -7.25}, "gcode": gcode("square_mm.nc")}

scenario(
    "stream_unbuffered_with_home_shift",
    "A short job without buffering: one line at a time, each after the previous ok. Absolute X/Y are shifted by "
    "home, keeping the decimals of both (10.5 + 12.3456 = 22.8456, 10.25 - 7.25 = 3.00); the comment line is "
    "skipped. The index resets when the file ends.",
    SHIFTED_HOME,
    [ui("startRun"), idle(), expect(state=FINISHED)],
)

scenario(
    "stream_buffered_with_home_shift",
    "A longer job with buffering on: lines go out while they fit in the controller's 126 bytes, counting the "
    "trailing space and newline; each ok frees the oldest line's bytes and lets the next line out.",
    {**SHIFTED_HOME, "gcode": gcode("long_cut_mm.nc"), "settings": BUFFERED},
    [ui("startRun"), idle(), expect(state=FINISHED)],
)


def slow_ok(free_space=True):
    '''
    A 4 s move: no ok, while the firmware's own reports, recorded mid-cut on the owner's machine, keep the link alive.
    free_space=False leaves out the [PE:] lines, whose free space (127) was recorded with nothing buffered.
    '''
    reports = owner_machine.CUTTING_REPORTS + owner_machine.CUTTING_REPORTS[:2]
    return [step for status_line, position_error in reports
            for step in (rx(status_line), *([rx(position_error)] if free_space else []),
                         tick(owner_machine.REPORT_INTERVAL))]


scenario(
    "stream_unbuffered_waits_for_a_slow_ok",
    "A long move's ok comes 4 s after the line, with status reports in between: nothing more goes out until it "
    "does, so no timer may release the next line.",
    SHIFTED_HOME,
    [ui("startRun"), wait_tx("X42.8456"), *slow_ok(), idle(), expect(state=FINISHED)],
)

scenario(
    "stream_buffered_waits_for_a_slow_ok",
    "Buffered: the buffer fills, then the oldest line's ok takes 4 s with status reports in between. Nothing more "
    "goes out until that ok frees its bytes.",
    {**MM, "gcode": gcode("long_cut_mm.nc"), "settings": BUFFERED},
    [ui("startRun"), wait_tx("X20 Y10"), firmware(ack=False), idle(), *slow_ok(free_space=False),
     firmware(ack=True), idle(),
     expect(state=FINISHED)],
)

scenario(
    "stream_buffered_short_job_fits_the_buffer",
    "The unbuffered job again with buffering on: all eleven lines (125 bytes with their trailing spaces and newlines) "
    "fit in the controller's 126, so they all go out before the first ok.",
    {**SHIFTED_HOME, "settings": BUFFERED},
    [ui("startRun"), idle(), expect(state=FINISHED)],
)

scenario(
    "start_mid_file_restores_modal_state",
    "Play at line 7 (G1 X25 Y25): restart sequence of units, G90, rapid to safe height and to the XY before the "
    "line, a G1 plunge at the file's plunge feed, then the file's G1 and feed restored, then the file from line 7.",
    {**MM, "gcode": gcode("restart_mm.nc"), "gcodeIndex": 7},
    [ui("startRun"), idle(), expect(state=FINISHED)],
)

scenario(
    "pause_jog_resume_returns_to_file_position",
    "Pause while a line is in flight: the line finishes and nothing more is sent. A jog while paused goes out; "
    "Resume returns to the file's XY at safe height, plunges back to the paused Z and carries on.",
    SHIFTED_HOME,
    [ui("startRun"), wait_tx("X42.8456"), ui("pauseRun", comment="the X42.8456 line is still unacknowledged"),
     idle(), expect(state={"uploadFlag": 2, "jobInProgress": True, "pauseButtonSetting": "Resume"}),
     ui("move", "right", "10"), idle(), ui("resumeRun"), idle(), expect(state=FINISHED)],
)

scenario(
    "m0_in_file_then_resume",
    "An M0 line stops streaming after it; the firmware prints Maslow Paused and holds the M0's ok. Resume sends ~, "
    "returns to the file position at safe height and carries on with the next line.",
    {**MM, "gcode": gcode("m0_pause_mm.nc")},
    [pauses_on(r"^M0", "Maslow Paused"), ui("startRun"), idle(),
     expect(state={"uploadFlag": 0, "jobInProgress": True, "gcodeIndex": 7, "pauseButtonSetting": "Resume"}),
     ui("resumeRun"), idle(), expect(state=FINISHED)],
)

# ----- tool change -----

scenario(
    "tool_change_t_and_m6_on_one_line",
    "T2 M6: streaming stops after the M6, the firmware asks for tool 2 and holds. Resume sends ~, returns to the "
    "file's XY at safe height without lowering the new bit, and streams the rest.",
    {**MM, "gcode": gcode("tool_change_mm.nc")},
    [pauses_on(r"M6", *TOOL_2), ui("startRun"), idle(),
     expect(state={"uploadFlag": 0, "jobInProgress": True, "manualZAxisAdjust": True, "currentTool": 2},
            emitted=[{"command": "activateModal", "data": {"message": "Please insert tool 2", "resume": "resume"}}]),
     ui("resumeRun"), idle(), expect(state={**FINISHED, "manualZAxisAdjust": False})],
)

scenario(
    "tool_change_t_then_m6_on_separate_lines",
    "T2 and M6 on separate lines: the firmware would run the lone T2 as an empty move and skip the M6, so "
    "WebControl does not send the T2 line and leads the M6 with T2.",
    {**MM, "gcode": {"lines": ["G21", "G90", "G0 X1", "T2", "M6", "G0 X2"]}},
    [pauses_on(r"M6", *TOOL_2), ui("startRun"), idle(), expect(state={"currentTool": 2, "uploadFlag": 0}),
     ui("resumeRun"), idle(), expect(state=FINISHED)],
)

scenario(
    "tool_change_m6_before_t_is_rewritten",
    "M6 T2, which the firmware stalls on, goes out as T2 M6.",
    {**MM, "gcode": {"lines": ["G21", "G90", "G0 X1", "M6 T2", "G0 X2"]}},
    [pauses_on(r"M6", *TOOL_2), ui("startRun"), idle(), expect(state={"currentTool": 2}),
     ui("resumeRun"), idle(), expect(state=FINISHED)],
)

scenario(
    "tool_change_jog_and_zero_z_then_resume",
    "During a tool change a Z move releases the firmware once with ~ and goes out; Define Z zero follows. Resume "
    "sends ~ again, returns to the file's XY at safe height and streams on.",
    {**MM, "gcode": gcode("tool_change_mm.nc")},
    [pauses_on(r"M6", *TOOL_2), ui("startRun"), idle(),
     ui("moveZ", "raise", "10"), idle(), ui("defineZ0"), idle(),
     expect(state={"manualZAxisAdjust": True, "uploadFlag": 0}),
     ui("resumeRun"), idle(), expect(state=FINISHED)],
)

# ----- stop and controller halts -----

scenario(
    "stop_mid_line_resets_the_index",
    "Stop while a line is in flight: ! goes out at once, the firmware flushes that line (no ok for it) and acks the "
    "!. The index resets, and commands wait for a status report before they go out again.",
    SHIFTED_HOME,
    [ui("startRun"), wait_tx("X42.8456"), ui("stopRun"), idle(), expect(state=FINISHED),
     ui("move", "up", "1"), *resync(), idle()],
)

scenario(
    "stop_ignores_a_status_report_read_too_soon",
    "After Stop the backend holds commands until it reads a status report once the input has run dry at least 0.25 s "
    "after the !; a report that comes right behind the !'s ok releases nothing.",
    SHIFTED_HOME,
    [ui("startRun"), wait_tx("X42.8456"), ui("stopRun"), idle(), status(42.85, 3, -1.5),
     ui("move", "up", "1", comment="held: the report above came within 0.25 s of the !"),
     expect(state=FINISHED), *resync(42.85, 3, -1.5), idle()],
    timing={"quiet": 0.05},
)

scenario(
    "stop_with_a_stale_ok_in_flight",
    "Stop races an ok already on the wire for the in-flight line: the stale ok and the !'s ok both arrive, and the "
    "backend still holds the next command until a status report shows the controller has caught up.",
    SHIFTED_HOME,
    [ui("startRun"), wait_tx("X42.8456"), firmware(forget=True), ui("stopRun"), wait_tx("^!"), firmware(forget=True),
     rx("ok", "the in-flight line finished just before the !"), rx("ok", "the !'s own ok"),
     ui("move", "up", "1", comment="held: the oks above may be stale"), *resync(), idle(), expect(state=FINISHED)],
)

scenario(
    "alarm_mid_cut_rewinds_and_resumes",
    "Buffered streaming; the firmware stops acking, raises ALARM and drops the lines it held. The index rewinds to "
    "the oldest unacknowledged line; after the status report Play resumes there, returning at safe height.",
    {**MM, "gcode": gcode("long_cut_mm.nc"), "settings": BUFFERED},
    [ui("startRun"), wait_tx("X30 Y20"), firmware(ack=False), idle(), rx(SLED_HALTED), firmware(flush=True, ack=True),
     expect(state={"uploadFlag": 0, "jobInProgress": True, "gcodeIndex": 9},
            emitted=[{"command": "alarm", "data": {"message": "Alarm: Sled Not Keeping Up"}}]),
     *resync(30, 20, -2), ui("startRun"), idle(), expect(state=FINISHED)],
)

HELD_LINES = ["G1 X%d Y%d  \\n" % xy for xy in [(20, 10), (20, 20), (30, 20), (30, 30), (40, 30), (40, 40),
                                                (50, 40), (50, 50), (60, 50)]]
# maslowRingBuffer::print(), which follows "Buffer overflow!" on 51.27.
OVERFLOW_DUMP = ["Buffer Used: 127", "Buffer Number of Lines: 9", "Buffer Begin: 1", "Buffer End: 0",
                 "Buffer Contents: " + "".join(HELD_LINES) + "(End of Buffer)"]

scenario(
    "buffer_overflow_halts_streaming",
    "The firmware reports Buffer overflow!: streaming halts, the index rewinds to the oldest unacknowledged line, "
    "and Stop then ends the job.",
    {**MM, "gcode": gcode("long_cut_mm.nc"), "settings": BUFFERED},
    [ui("startRun"), wait_tx("X20 Y10"), firmware(ack=False), idle(), rx("Buffer overflow!"),
     *[rx(line) for line in OVERFLOW_DUMP], rx("ok", "the firmware runs the unread tail of the line and acks it"),
     firmware(flush=True, ack=True),
     expect(state={"uploadFlag": 0, "jobInProgress": True, "gcodeIndex": 7},
            emitted=[{"command": "alarm", "data": {"message": "Alarm: Controller Buffer Overflow"}}]),
     *resync(20, 10, -2), ui("stopRun"), idle(), *resync(20, 10, -2), expect(state=FINISHED)],
)

# ----- lines the controller cannot take -----

LONG_LINE = ("G1 X1 Y1 " + "F100 " * 24).rstrip()

scenario(
    "line_too_long_pauses_the_job",
    "A line over 126 bytes as written would overflow the firmware's buffer and run its tail as a command, so the job "
    "pauses before it with an alert; nothing of it is sent.",
    {**MM, "gcode": {"lines": ["G0 X1", LONG_LINE, "G0 X2"]}},
    [ui("startRun"), idle(),
     expect(state={"uploadFlag": 2, "jobInProgress": True, "gcodeIndex": 1},
            alerts=[{"title": "Line Too Long", "message": "Line 2 is 131 characters"}])],
)

scenario(
    "unclosed_comment_pauses_the_job",
    "A line with an unclosed ( would hang 51.27, so the job pauses before it with an alert.",
    {**MM, "gcode": {"lines": ["G0 X1", "G1 X2 (rough pass", "G0 X3"]}},
    [ui("startRun"), idle(),
     expect(state={"uploadFlag": 2, "gcodeIndex": 1}, alerts=[{"title": "Line Unclosed Comment"}])],
)

scenario(
    "realtime_characters_stripped_from_lines",
    "!, ~, backtick, ? and control characters act as realtime commands anywhere in a line, so they are removed from "
    "streamed lines; closed comments are removed too.",
    {**MM, "gcode": {"lines": ["G1 X1! Y2~ Z`3? F4\u0018", "G0 X5 (note) ; trailing"]}},
    [ui("startRun"), idle(), expect(state=FINISHED)],
)

# ----- firmware-specific commands -----

scenario(
    "z_limits_refused_on_51_27",
    "51.27 stores no Z limits (B17-B19 are Invalid statement), so every Z-limit action alerts and sends nothing.",
    {**MM},
    [ui(command) for command in ("setMaxZ", "setMinZ", "clearZ", "getZlimits")]
    + [expect(alerts=[{"message": "Z-axis limits need holey firmware 51.292 or later"}])],
)

scenario(
    "z_limits_on_51_292",
    "51.292 keeps Z limits as $63/$64 in mm: Set Max and Set Min write the current Z, Clear writes far-off limits, "
    "and reading them sends B20. Each $ write is answered as 51.292 answers it: the value echoed, the new limit, and "
    "the EEPROM rewrite notice.",
    {**MM},
    [expect(state={"firmwareBuild": "51.292"}), status(0, 0, 12.34), ui("setMaxZ"), idle(),
     status(0, 0, -20), ui("setMinZ"), idle(), ui("clearZ"), idle(),
     firmware(rule={"pattern": r"^B20", "replies": ["Z Axis Upper Limit: 9999.00 Lower Limit: -9999.00"]}),
     ui("getZlimits"), idle()],
    firmware_meta={"variant": "holey", "version": "51.29", "build": "51.292"},
)

HOLEY_MEASUREMENTS = [966.0, 964.5, 965.8, 964.9, 711.6, 710.9, 711.4, 1199.5, 1198.2, 1199.1, 1198.6, 254.3]

scenario(
    "holey_calibration_accept_writes_settings",
    "Holey calibration: Calculate sends nothing; Accept stores the fitted motor spacing, motor offset and chain "
    "tolerances and writes each to the controller as its $ key.",
    {**MM},
    [http("/api/calibration/holey", json={"measurements": HOLEY_MEASUREMENTS}, status=200), idle(),
     http("/api/calibration/holey/accept", json={}, status=200), idle()],
)

# ----- real jobs from the owner's machine -----

import real_job_scenarios  # noqa: E402,F401  registers its scenarios, built from the helpers above
