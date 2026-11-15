"""Tool changes stop streaming exactly where the firmware pauses, and only Resume releases the firmware."""
import pytest

from Connection.firmwareGcode import toolTheFirmwareReads
from Connection.serialPortThread import SerialPortThread
from helpers import drain
from test_job_in_progress import action, from_controller, job  # noqa: F401
from test_serial_streaming import once_after, serial  # noqa: F401

TOOL_CHANGE = ["Tool Change: Please insert tool 2\r\n", "Maslow Paused\r\n"]


@pytest.fixture
def tools(job, serial):  # noqa: F811
    saved = {key: getattr(job, key) for key in ("nextTool", "firmwarePaused", "firmwareHoldsJobLines", "toolChangeWaits", "units", "toolBeforeChange")}
    job.currentTool = 0
    job.nextTool = 0
    job.firmwarePaused = False
    job.toolChangeWaits = False
    job.firmwareHoldsJobLines = False
    job.toolBeforeChange = None
    job.manualZAxisAdjust = False
    yield job
    for key, value in saved.items():
        setattr(job, key, value)


def stream(data, serial, lines):  # noqa: F811
    """Starts a job on lines and sends until streaming stops; returns what reached the port."""
    data.gcode = lines
    data.gcodeIndex = 0
    action(data, "startRun")
    thread = serial.thread()
    while data.uploadFlag == 1:
        thread.sendNextLine()
    drain(data.gcode_queue)
    return serial.port.sent


def controller_says(app, data, messages):
    [from_controller(app, data, message) for message in messages]


def test_t_on_its_own_line_is_not_sent_and_leads_the_m6_the_firmware_would_skip(tools, serial):  # noqa: F811
    sent = stream(tools, serial, ["G1 X1 ", "T2 ", "M6 ", "G1 X2 "])

    assert sent == ["G1 X1  ", "T2 M6  "]
    assert (tools.uploadFlag, tools.currentTool, tools.gcodeIndex) == (-1, 2, 3)


@pytest.mark.parametrize("line", ["T2 ", "T 3 ", "N10 T2 ", "T2.0 "])
def test_a_line_that_only_selects_a_tool_is_not_sent(tools, serial, line):  # noqa: F811
    sent = stream(tools, serial, ["G1 X1 ", line, "G1 X2 "])

    assert sent == ["G1 X1  ", "G1 X2  "]
    assert tools.nextTool in (2, 3)


def test_a_queued_lone_t_is_not_sent_but_selects_the_tool(tools, serial):  # noqa: F811
    tools.gcode_queue.put("T2")
    tools.gcode_queue.put("M6")
    port = serial.run()

    assert "T2 " not in port.sent and "T2 M6 " in port.sent


def test_m6_before_its_t_is_sent_t_first_so_the_firmware_changes_tool(tools, serial):  # noqa: F811
    sent = stream(tools, serial, ["M6 T2 ", "G1 X2 "])

    assert sent == ["T2 M6  "]
    assert (tools.uploadFlag, tools.currentTool) == (-1, 2)


def test_m06_counts_as_a_tool_change(tools, serial):  # noqa: F811
    stream(tools, serial, ["T3 M06 ", "G1 X2 "])

    assert (tools.uploadFlag, tools.currentTool) == (-1, 3)


def test_m6_for_the_tool_already_in_keeps_streaming(tools, serial):  # noqa: F811
    tools.currentTool = 2

    sent = stream(tools, serial, ["T2 M6 ", "G1 X2 "])

    assert sent == ["T2 M6  ", "G1 X2  "]
    assert tools.uploadFlag == 0


def test_a_t_after_a_g_word_still_selects_the_tool_for_the_next_m6(tools, serial):  # noqa: F811
    sent = stream(tools, serial, ["G0 X1 T2 ", "M6 ", "G1 X2 "])

    assert sent == ["G0 X1 T2  ", "T2 M6  "]
    assert (tools.uploadFlag, tools.currentTool) == (-1, 2)


def test_a_line_number_before_the_t_does_not_hide_the_tool_change(tools, serial):  # noqa: F811
    sent = stream(tools, serial, ["N10 T2 M6 ", "G1 X2 "])

    assert sent == ["T2 N10 M6  "]
    assert (tools.uploadFlag, tools.currentTool) == (-1, 2)


@pytest.mark.parametrize(
    "line, tool",
    [
        ("T2 M6 ", 2), ("T2.0 M6 ", 2), ("T 2 M6 ", 0), ("t3 g0 x1 ", 3), ("T2 ", None), ("M6 T2 ", None),
        ("N10 T2 M6 ", None), ("G0 X1 T2 ", None), ("B14 T2 M6 ", None),
    ],
)
def test_the_firmware_reads_t_only_where_its_parser_does(line, tool):
    assert toolTheFirmwareReads(line) == tool


def test_a_tool_change_line_never_overfills_the_controller_buffer(tools, serial):  # noqa: F811
    tools.gcode = ["T2 "] + ["G1 X1 Y1 "] * 10 + ["X1 ", "M6 ", "G1 X2 "]
    tools.uploadFlag = 1
    skip_startup_commands = once_after(serial.clock, 0, lambda port: drain(tools.gcode_queue))

    port = serial.run(ack=False, on_poll=skip_startup_commands, buffer=True)

    assert sum(len(line) for line in port.written) <= SerialPortThread.bufferSize


def test_m60_and_b14_t_words_are_not_tool_changes(tools, serial):  # noqa: F811
    sent = stream(tools, serial, ["M60 ", "B14 L1 S0 F1 I1 T2 V1 ", "M6 ", "G1 X2 "])

    assert len(sent) == 4
    assert (tools.currentTool, tools.nextTool) == (0, 0)


def test_the_tool_change_message_leaves_the_firmware_paused(app, tools, serial):  # noqa: F811
    stream(tools, serial, ["T2 M6 ", "G1 X2 "])

    controller_says(app, tools, TOOL_CHANGE)

    assert drain(tools.quick_queue) == []
    assert (tools.uploadFlag, tools.jobInProgress, tools.manualZAxisAdjust) == (0, True, True)


def test_resume_continues_the_job_whichever_order_the_processors_ran_in(app, tools, serial):  # noqa: F811
    stream(tools, serial, ["T2 M6 ", "G1 X2 "])
    [tools.messageProcessor.processMessage(message) for message in TOOL_CHANGE]
    from_controller(app, tools, "")

    action(tools, "resumeRun")

    assert drain(tools.quick_queue) == ["~"]
    assert (tools.uploadFlag, tools.manualZAxisAdjust, tools.gcodeIndex) == (1, False, 1)


def test_a_tool_change_outside_a_job_resumes_without_starting_the_file(app, tools, serial):  # noqa: F811
    tools.gcode = ["G1 X1 ", "G1 X2 "]
    tools.uploadFlag = 0
    tools.jobInProgress = False
    serial.thread().manageToolChange("T2 M6 ")
    controller_says(app, tools, TOOL_CHANGE)

    action(tools, "resumeRun")

    assert (tools.uploadFlag, tools.manualZAxisAdjust) == (0, False)
    assert drain(tools.quick_queue) == ["~"]


def test_jogging_during_a_tool_change_releases_the_firmware_once(app, tools, serial):  # noqa: F811
    stream(tools, serial, ["T2 M6 ", "G1 X2 "])
    controller_says(app, tools, TOOL_CHANGE)
    assert drain(tools.quick_queue) == []

    tools.actions.processAction({"data": {"command": "moveZ", "arg": "raise", "arg1": "5"}})
    tools.actions.processAction({"data": {"command": "moveZ", "arg": "lower", "arg1": "1"}})

    assert drain(tools.quick_queue) == ["~"]
    assert (tools.uploadFlag, tools.manualZAxisAdjust, tools.firmwarePaused) == (0, True, False)


def test_stop_during_a_tool_change_clears_the_paused_firmware(app, tools, serial):  # noqa: F811
    stream(tools, serial, ["T2 M6 ", "G1 X2 "])
    controller_says(app, tools, TOOL_CHANGE)

    action(tools, "stopRun")

    assert (tools.firmwarePaused, tools.manualZAxisAdjust, tools.jobInProgress) == (False, False, False)


def test_a_reconnect_forgets_the_tool_the_firmware_had_queued(tools, serial):  # noqa: F811
    tools.nextTool = 4
    tools.currentTool = 4

    serial.run()

    assert (tools.currentTool, tools.nextTool) == (0, 0)


def restart_tool_change(data, lines):
    data.gcode = lines
    data.gcodeIndex = len(lines)
    data.actions.processGCode()
    return [line for line in drain(data.gcode_queue) if "M6" in line]


def test_a_restart_after_t_and_m6_on_separate_lines_changes_to_that_tool(tools):
    assert restart_tool_change(tools, ["G21 ", "T2 ", "M6 ", "G0 X1 "]) == ["T2 M6 "]


def test_a_restart_after_m6_t3_changes_to_tool_3_as_the_rewritten_line_did(tools):
    assert restart_tool_change(tools, ["G21 ", "M6 T3 ", "G0 X1 "]) == ["T3 M6 "]


def test_a_restart_does_not_change_to_a_tool_only_named_so_far(tools):
    assert restart_tool_change(tools, ["G21 ", "T2 ", "M6 ", "T5 ", "G0 X1 "]) == ["T2 M6 "]


def test_the_firmwares_tool_change_message_corrects_the_tracked_tool(app, tools):
    tools.currentTool = 2
    tools.nextTool = 2

    controller_says(app, tools, ["Tool Change: Please insert tool 5\r\n"])

    assert (tools.currentTool, tools.nextTool) == (5, 5)


def test_a_restart_after_a_t_behind_a_g_word_changes_to_that_tool(tools):
    assert restart_tool_change(tools, ["G21 ", "G0 X1 T4 ", "M6 ", "G0 X1 "]) == ["T4 M6 "]


def firmware_that_pauses_on_m6(app, data, on_pause):
    """Acks each line, except that an M6 holds its ok until "~", the way pause() blocks the line."""
    state = {"seen": 0, "held": False, "started": False}

    def poll(port):
        if not state["started"]:
            state["started"] = True
            drain(data.gcode_queue)
        written = port.sent[state["seen"]:]
        state["seen"] = len(port.sent)
        for line in written:
            if line == "~":
                port.incoming.extend(["ok\r\n"] * (2 if state["held"] else 1))
                state["held"] = False
            elif "M6" in line:
                state["held"] = True
                [data.messageProcessor.processMessage(message) for message in TOOL_CHANGE]
                on_pause()
            else:
                port.incoming.append("ok\r\n")

    return poll


TOOL_CHANGE_JOB = [
    "G21 ", "G90 ", "G0 Z5 ", "G0 X10 Y10 ", "G1 Z-3 F100 ", "G1 X20 F600 ", "G0 Z5 ", "T2 M6 ", "G0 X40 Y40 ",
    "G1 Z-2 F100 ", "X50 ",
]
AFTER_THE_M6 = ["G0 X40 Y40  ", "G1 Z-2 F100  ", "X50  "]
RETURN_AT_SAFE_HEIGHT = ["G21  ", "G90  ", "G0 Z5.0  ", "G0 X20.0 Y10.0  ", "G0 F600.0  ", "G90  "]


def run_tool_change_job(app, data, serial, during_the_pause):  # noqa: F811
    data.gcode = list(TOOL_CHANGE_JOB)
    data.gcodeIndex = 0
    data.uploadFlag = 1
    data.jobInProgress = True
    data.units = "MM"
    port = serial.run(on_poll=firmware_that_pauses_on_m6(app, data, during_the_pause))
    return port.sent[port.sent.index("T2 M6  ") + 1:]


def test_resume_after_a_tool_change_releases_the_firmware_before_the_return_moves(app, tools, serial):  # noqa: F811
    sent = run_tool_change_job(app, tools, serial, lambda: action(tools, "resumeRun"))

    assert sent == ["~"] + RETURN_AT_SAFE_HEIGHT + AFTER_THE_M6


def test_resume_after_jogging_during_a_tool_change_returns_to_the_file_at_safe_height(app, tools, serial):  # noqa: F811
    def jog_then_resume():
        tools.actions.processAction({"data": {"command": "moveTo", "arg": 0, "arg1": 0}})
        action(tools, "resumeRun")

    sent = run_tool_change_job(app, tools, serial, jog_then_resume)

    assert sent == ["~", "~", "G90 G00 X0.0 Y0.0  "] + RETURN_AT_SAFE_HEIGHT + AFTER_THE_M6


def test_resume_after_a_tool_change_never_lowers_the_new_bit_to_a_z_from_an_old_pause(app, tools, serial):  # noqa: F811
    tools.pausedGcodeIndex = TOOL_CHANGE_JOB.index("T2 M6 ") + 1
    tools.pausedzval = -3.0

    sent = run_tool_change_job(app, tools, serial, lambda: action(tools, "resumeRun"))

    assert not any(line.startswith(("G1 Z-3", "G0 Z-3")) for line in sent)


def test_a_jog_never_releases_job_lines_the_firmware_holds_behind_an_unexpected_pause(app, tools, serial):  # noqa: F811
    tools.gcode = ["G1 X1 ", "G1 Z-3 ", "G1 X2 ", "G1 X3 "]
    tools.gcodeIndex = 0
    action(tools, "startRun")
    thread = serial.thread()
    [thread.sendNextLine() for _ in range(3)]
    drain(tools.ui_queue1)

    controller_says(app, tools, ["Message: Please adjust Z-Axis to a depth of -3.00 mm\r\n", "Maslow Paused\r\n"])
    tools.actions.processAction({"data": {"command": "move", "arg": "right", "arg1": "10"}})

    assert drain(tools.quick_queue) == []
    assert drain(tools.gcode_queue) == []
    assert tools.firmwarePaused is True


def test_resume_releases_the_held_job_lines_and_lets_jogs_release_the_next_pause(app, tools, serial):  # noqa: F811
    tools.gcode = ["G1 X1 ", "G1 Z-3 ", "G1 X2 "]
    tools.gcodeIndex = 0
    action(tools, "startRun")
    serial.thread().sendNextLine()
    controller_says(app, tools, ["Message: Please adjust Z-Axis to a depth of -3.00 mm\r\n", "Maslow Paused\r\n"])
    action(tools, "resumeRun")
    drain(tools.quick_queue)

    tools.uploadFlag = 0
    controller_says(app, tools, ["Maslow Paused\r\n"])
    tools.actions.processAction({"data": {"command": "move", "arg": "right", "arg1": "10"}})

    assert drain(tools.quick_queue) == ["~"]


def test_starting_mid_file_after_a_tool_change_queues_nothing_behind_its_m6(app, tools, serial):  # noqa: F811
    tools.gcode = list(TOOL_CHANGE_JOB)
    tools.gcodeIndex = len(TOOL_CHANGE_JOB) - 1
    tools.units = "MM"

    def jog_then_resume():
        tools.actions.processAction({"data": {"command": "moveZ", "arg": "raise", "arg1": "5"}})
        action(tools, "resumeRun")

    pauses = firmware_that_pauses_on_m6(app, tools, jog_then_resume)
    started = []

    def start_mid_file(port):
        pauses(port)
        if not started:
            started.append(True)
            action(tools, "startRun")

    port = serial.run(on_poll=start_mid_file)

    sent = port.sent
    assert sent[:6] == ["G21  ", "G90  ", "G0 Z5.0  ", "G0 X40.0 Y40.0  ", "T2 M6  ", "~"]
    assert [line for line in sent if line.startswith("G1 Z")] == ["G1 Z-2.0 F100.0  "]
    assert sent.index("G00 Z127.0  ") < sent.index("G1 Z-2.0 F100.0  "), "the Z panel defaults to inches"
    assert sent[-6:] == ["G0 Z5.0  ", "G0 X40.0 Y40.0  ", "G1 Z-2.0 F100.0  ", "G1 F100.0  ", "G90  ", "X50  "]


def test_a_jog_does_not_confirm_a_manual_z_adjustment_outside_a_job(app, tools, serial):  # noqa: F811
    tools.uploadFlag = 0
    tools.jobInProgress = False
    controller_says(app, tools, ["Message: Please adjust Z-Axis to a depth of -3.00 mm\r\n", "Maslow Paused\r\n"])
    drain(tools.ui_queue1)

    tools.actions.processAction({"data": {"command": "move", "arg": "right", "arg1": "10"}})

    assert drain(tools.quick_queue) == []
    assert drain(tools.gcode_queue) == []


def test_resume_after_a_manual_z_prompt_carries_on_with_the_line_without_moving(app, tools, serial):  # noqa: F811
    tools.gcode = ["G1 X1 ", "G1 Z-3 ", "G1 X2 "]
    tools.gcodeIndex = 0
    action(tools, "startRun")
    thread = serial.thread()
    [thread.sendNextLine() for _ in range(2)]
    controller_says(app, tools, ["Message: Please adjust Z-Axis to a depth of -3.00 mm\r\n", "Maslow Paused\r\n"])

    action(tools, "resumeRun")

    assert drain(tools.quick_queue) == ["~"]
    assert drain(tools.gcode_queue) == []
    assert (tools.uploadFlag, tools.gcodeIndex, tools.manualZAxisAdjust) == (1, 2, False)


def test_a_reconnect_forgets_that_the_firmware_was_paused(tools, serial):  # noqa: F811
    tools.firmwarePaused = True
    tools.firmwareHoldsJobLines = True

    serial.run()

    assert (tools.firmwarePaused, tools.firmwareHoldsJobLines) == (False, False)


def test_an_m0_or_m6_outside_a_job_does_not_hold_streaming(tools, serial):  # noqa: F811
    tools.uploadFlag = 0
    tools.jobInProgress = False
    thread = serial.thread()

    thread.managePause("M0 ")
    thread.manageToolChange("T2 M6 ")

    assert tools.uploadFlag == 0


def test_an_m6_for_the_tool_already_in_still_names_it_for_the_firmware(tools, serial):  # noqa: F811
    tools.currentTool = 1
    tools.nextTool = 1

    sent = stream(tools, serial, ["T3 G0 X1 ", "T1 ", "M6 ", "G1 X2 "])

    assert sent == ["T3 G0 X1  ", "T1 M6  ", "G1 X2  "]
    assert tools.uploadFlag == 0


def test_stop_before_the_firmware_reaches_the_m6_keeps_the_tool_it_had(tools, serial):  # noqa: F811
    stream(tools, serial, ["T2 M6 ", "G1 X2 "])

    action(tools, "stopRun")

    assert (tools.currentTool, tools.nextTool) == (0, 0)
    sent = stream(tools, serial, ["T2 M6 ", "G1 X2 "])
    assert sent[-1] == "T2 M6  "
    assert tools.uploadFlag == -1


def test_stop_after_the_firmware_confirmed_the_tool_change_keeps_the_new_tool(app, tools, serial):  # noqa: F811
    stream(tools, serial, ["T2 M6 ", "G1 X2 "])
    controller_says(app, tools, TOOL_CHANGE)

    action(tools, "stopRun")

    assert (tools.currentTool, tools.nextTool) == (2, 2)


def test_the_t_added_to_an_m6_can_push_it_over_the_controllers_limit(tools, serial):  # noqa: F811
    long_m6 = "M6" + " " * 121  # 125 bytes as written, 128 once led by "T2 "

    sent = stream(tools, serial, ["T2 ", long_m6, "G1 X2 "])

    assert sent == []
    assert (tools.uploadFlag, tools.currentTool) == (2, 0)


def test_a_controller_boot_forgets_its_pause_and_tool(app, tools, monkeypatch):
    tools.firmwarePaused = tools.firmwareHoldsJobLines = True
    tools.currentTool = tools.nextTool = 3
    monkeypatch.setattr(tools, "bootInProgress", False)
    monkeypatch.setattr(tools, "lastBootBannerTime", tools.lastBootBannerTime)

    controller_says(app, tools, ["Grbl v1.00\r\n"])

    assert (tools.firmwarePaused, tools.firmwareHoldsJobLines, tools.currentTool, tools.nextTool) == (False, False, 0, 0)
