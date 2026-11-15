"""M0/M1 stop streaming where the firmware pauses; the firmware waits for Resume's "~" and the job carries on."""
import pytest

from helpers import drain
from test_job_in_progress import action, from_controller, job  # noqa: F401
from test_serial_streaming import serial  # noqa: F401
from test_tool_change import stream, tools  # noqa: F401

FILE = ["G21 ", "G90 ", "G0 Z5 ", "G0 X10 Y10 ", "G1 Z-3 F100 ", "G1 X20 F300 ", "M0", "G1 X30 "]
AFTER_PAUSE = FILE.index("M0") + 1


@pytest.mark.parametrize("line", ["M0", "M1", "M00 ", "M01", "G1 X2 M0", "M1 G1 X2 "])
def test_m0_and_m1_stop_streaming_after_their_line(tools, serial, line):  # noqa: F811
    sent = stream(tools, serial, ["G1 X1 ", line, "G1 X3 "])

    assert len(sent) == 2
    assert (tools.uploadFlag, tools.gcodeIndex) == (-1, 2)


@pytest.mark.parametrize("line", ["M30 ", "M10 ", "M100 ", "G1 X1.0 "])
def test_longer_m_codes_do_not_stop_streaming(tools, serial, line):  # noqa: F811
    sent = stream(tools, serial, ["G1 X1 ", line, "G1 X3 "])

    assert len(sent) == 3


def test_m0_does_not_release_the_firmware_before_it_pauses(app, tools, serial):  # noqa: F811
    stream(tools, serial, FILE)

    from_controller(app, tools, "Maslow Paused\r\n")

    assert drain(tools.quick_queue) == []
    assert (tools.uploadFlag, tools.jobInProgress, tools.firmwarePaused) == (0, True, True)


def test_resume_after_m0_releases_the_firmware_and_carries_on_from_the_next_line(app, tools, serial):  # noqa: F811
    stream(tools, serial, FILE)
    from_controller(app, tools, "Maslow Paused\r\n")

    action(tools, "resumeRun")

    assert drain(tools.quick_queue) == ["~"]
    assert drain(tools.gcode_queue) == [
        "G21 ", "G90 ", "G0 Z5.0 ", "G0 X20.0 Y10.0 ", "G1 Z-3.0 F100.0 ", "F300.0 ", "G90 ",
    ]
    assert (tools.uploadFlag, tools.gcodeIndex, tools.firmwarePaused) == (1, AFTER_PAUSE, False)
    serial.thread().sendNextLine()
    assert serial.port.sent == ["G1 X30  "]


def test_resume_after_an_m0_sent_outside_a_job_does_not_start_the_file(app, tools, serial):  # noqa: F811
    tools.gcode = list(FILE)
    tools.gcodeIndex = 0
    tools.uploadFlag = 0
    tools.jobInProgress = False
    from_controller(app, tools, "Maslow Paused\r\n")

    action(tools, "resumeRun")

    assert drain(tools.quick_queue) == ["~"]
    assert drain(tools.gcode_queue) == []
    assert (tools.uploadFlag, tools.firmwarePaused) == (0, False)
