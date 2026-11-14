"""Pins current Actions.processAction behaviour so the new UI can rely on the same command names and side effects."""
import pytest

from helpers import decode_ui_message, drain


def action(command, arg="", arg1=""):
    return {"data": {"command": command, "arg": arg, "arg1": arg1}}


@pytest.fixture
def home(data):
    saved = [data.config.getValue("Advanced Settings", key) for key in ("homeX", "homeY")]
    saved_shift = list(data.gcodeShift)
    yield
    data.config.setValue("Advanced Settings", "homeX", saved[0], recursionBreaker=True)
    data.config.setValue("Advanced Settings", "homeY", saved[1], recursionBreaker=True)
    data.gcodeShift = saved_shift


def ui_messages(data):
    return [decode_ui_message(raw) for raw in drain(data.ui_queue1)]


def test_define_home_without_coordinates_uses_the_sled_position(data, home):
    data.uploadFlag = 0
    data.xval, data.yval = 101.23456, -55.5

    data.actions.processAction(action("defineHome"))

    assert data.config.getValue("Advanced Settings", "homeX") == "101.2346"
    assert data.config.getValue("Advanced Settings", "homeY") == "-55.5"
    assert data.gcodeShift == [101.2346, -55.5]
    assert ui_messages(data) == [
        ("Action", "homePositionMessage", {"xval": 101.2346, "yval": -55.5}),
        ("Action", "unitsUpdate", ""),
        ("Action", "gcodeUpdate", ""),
    ]


def test_define_home_with_coordinates_scales_inches_to_mm(data, home):
    data.uploadFlag = 0
    data.units = "MM"

    data.actions.processAction(action("defineHome", 2, 1))

    assert data.gcodeShift == [50.8, 25.4]


def test_move_queues_a_relative_rapid_and_remembers_the_distance(data):
    data.uploadFlag = 0

    data.actions.processAction(action("move", "up", "10"))

    assert drain(data.gcode_queue) == ["G91 ", "G00 Y10.0 ", "G90 "]
    assert data.config.getValue("Computed Settings", "distToMove") == 10.0


@pytest.mark.parametrize(
    "direction, gcode",
    [("left", "G00 X-5.0 "), ("right", "G00 X5.0 "), ("down", "G00 Y-5.0 "), ("upLeft", "G00 X-5.0 Y5.0 ")],
)
def test_move_directions(data, direction, gcode):
    data.uploadFlag = 0
    data.config.setValue("WebControl Settings", "diagonalMove", 0, recursionBreaker=True)
    data.actions.processAction(action("move", direction, "5"))
    assert drain(data.gcode_queue)[1] == gcode


def test_move_is_refused_while_gcode_is_running(data):
    data.uploadFlag = 1

    data.actions.processAction(action("move", "up", "10"))

    assert drain(data.gcode_queue) == []
    assert ui_messages(data) == [("Alert", "Alert", "Cannot issue command while sending gcode.")]


def test_stop_is_allowed_while_gcode_is_running(data):
    data.uploadFlag = 1
    data.actions.processAction(action("stopRun"))
    assert data.uploadFlag == 0


def test_unknown_commands_raise_an_alert(data):
    data.uploadFlag = 0
    data.actions.processAction(action("noSuchThing"))
    assert ui_messages(data) == [("Alert", "Alert", "Function not currently implemented.. Sorry.[noSuchThing]")]
