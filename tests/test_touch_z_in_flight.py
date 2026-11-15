"""While touchZ's probe descends nothing may move the machine; only Stop gets through."""
import pytest

from helpers import decode_ui_message, drain
from test_serial_streaming import serial  # noqa: F401

PROBING_MESSAGE = "Wait for Touch Z to finish or press Stop"
MOVING_ACTIONS = [
    ("resumeRun", "", ""), ("startRun", "", ""), ("move", "right", "10"), ("moveZ", "raise", "5"), ("home", "", ""),
    ("returnToCenter", "", ""), ("touchZ", "", ""), ("defineZ0", "", ""), ("macro1", "", ""),
    ("moveToDefault", "", ""), ("testMotors", "", ""), ("cutHoleyCalibrationPattern", "", ""),
]


@pytest.fixture
def probing(data, monkeypatch):
    monkeypatch.setattr(data, "probeInFlight", False, raising=False)
    data.uploadFlag, data.jobInProgress = 2, True
    act(data, "touchZ")
    drain(data.gcode_queue)
    drain(data.ui_queue1)
    return data


def act(data, command, arg="", arg1=""):
    return data.actions.processAction({"data": {"command": command, "arg": arg, "arg1": arg1}})


def alerts(data):
    return [text for command, _, text in map(decode_ui_message, drain(data.ui_queue1)) if command == "Alert"]


def test_touch_z_marks_a_probe_in_flight(probing):
    assert probing.probeInFlight is True


@pytest.mark.parametrize("command, arg, arg1", MOVING_ACTIONS)
def test_a_moving_action_mid_probe_is_refused_with_an_alert(probing, command, arg, arg1):
    act(probing, command, arg, arg1)

    assert alerts(probing) == [PROBING_MESSAGE]
    assert drain(probing.gcode_queue) == []
    assert drain(probing.quick_queue) == []
    assert (probing.uploadFlag, probing.jobInProgress) == (2, True)


def test_typed_gcode_mid_probe_is_refused(probing):
    assert probing.actions.sendGCode("G0 X10") is False

    assert alerts(probing) == [PROBING_MESSAGE]
    assert drain(probing.gcode_queue) == []


@pytest.mark.parametrize("stop", ["stopRun", "stopZ"])
def test_stop_mid_probe_goes_out_and_ends_the_probe(probing, stop):
    act(probing, stop)

    assert drain(probing.quick_queue) == ["!"]
    assert probing.probeInFlight is False


@pytest.mark.parametrize("answer", ["ok", "error: probe did not connect"])
def test_the_probes_own_answer_ends_the_probe(serial, data, monkeypatch, answer):  # noqa: F811
    monkeypatch.setattr(data, "probeInFlight", False, raising=False)
    thread = serial.thread(ack=False)
    thread.machineIsReadyForData = True
    serial.port.incoming.clear()

    thread._sendQueuedCommand("G90 G38.2 Z-10")
    assert data.probeInFlight is True
    serial.port.incoming.append(answer + "\r\n")
    thread._exchangeLines()

    assert data.probeInFlight is False


def test_a_halt_ends_the_probe(serial, data, monkeypatch):  # noqa: F811
    monkeypatch.setattr(data, "probeInFlight", True, raising=False)
    thread = serial.thread()

    thread._awaitResync()

    assert data.probeInFlight is False


def test_a_new_connection_forgets_a_probe(serial, data, monkeypatch):  # noqa: F811
    monkeypatch.setattr(data, "probeInFlight", True, raising=False)

    serial.run()

    assert data.probeInFlight is False
