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
    saved = {key: getattr(job, key) for key in ("nextTool", "firmwarePaused")}
    job.currentTool = 0
    job.nextTool = 0
    job.firmwarePaused = False
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


def test_t_on_its_own_line_then_m6_sends_the_t_with_the_m6_the_firmware_would_skip(tools, serial):  # noqa: F811
    sent = stream(tools, serial, ["G1 X1 ", "T2 ", "M6 ", "G1 X2 "])

    assert sent == ["G1 X1  ", "T2  ", "T2 M6  "]
    assert (tools.uploadFlag, tools.currentTool, tools.gcodeIndex) == (-1, 2, 3)


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
