"""Loading or rendering a gcode file must leave the machine's units, home and gcode shift alone."""
import pytest

from helpers import decode_ui_message, drain


@pytest.fixture
def inch_machine(data):
    config = data.config
    keys = [("Advanced Settings", "homeX"), ("Advanced Settings", "homeY"), ("Computed Settings", "units"),
            ("Computed Settings", "distToMove"), ("Computed Settings", "tolerance")]
    saved_config = [config.getValue(*key) for key in keys]
    saved = {name: getattr(data, name) for name in ["gcode", "gcodeIndex", "gcodeShift", "gcodeFileUnits", "tolerance"]}
    data.units = "INCHES"
    config.setValue("Computed Settings", "units", "INCHES", recursionBreaker=True)
    config.setValue("Advanced Settings", "homeX", 2.0, recursionBreaker=True)
    config.setValue("Advanced Settings", "homeY", 3.0, recursionBreaker=True)
    data.gcodeShift = [2.0, 3.0]
    data.gcodeIndex = 0
    drain(data.ui_queue1)
    yield data
    for key, value in zip(keys, saved_config):
        config.setValue(*key, value, recursionBreaker=True)
    for name, value in saved.items():
        setattr(data, name, value)
    data.gcodeFile.line3D[:] = []


class InlineThread:
    def __init__(self, target):
        self.target = target

    def start(self):
        self.target()


@pytest.fixture(autouse=True)
def render_inline(monkeypatch):
    monkeypatch.setattr("File.gcodeFile.threading.Thread", InlineThread)


def render(data, text):
    data.gcodeFile.loadUpdateFile(text)


def test_rendering_a_metric_file_leaves_an_inch_machine_in_inches(inch_machine):
    render(inch_machine, "G21\nG0 X25.4 Y25.4\nG1 Z-1\n")

    assert inch_machine.units == "INCHES"
    assert inch_machine.config.getValue("Computed Settings", "units") == "INCHES"
    assert float(inch_machine.config.getValue("Advanced Settings", "homeX")) == 2.0
    assert inch_machine.gcodeShift == [2.0, 3.0]
    assert drain(inch_machine.gcode_queue) == []
    commands = [decode_ui_message(raw)[1] for raw in drain(inch_machine.ui_queue1)]
    assert "unitsUpdate" not in commands
    assert inch_machine.gcodeFileUnits == "MM"


def test_a_metric_file_still_renders_in_inches(inch_machine):
    render(inch_machine, "G21\nG1 X25.4 Y50.8\n")

    assert inch_machine.gcodeFile.line3D[-1].points[-1] == pytest.approx((1.0, 2.0, 0))


def test_starting_a_loaded_metric_file_mid_way_switches_the_machine_with_g21(inch_machine):
    render(inch_machine, "G21\nG0 X25.4 Y25.4\nG1 Z-1\n")
    inch_machine.gcodeIndex = 2

    inch_machine.actions.processGCode()

    assert drain(inch_machine.gcode_queue)[:4] == ["G21 ", "G90 ", "G0 Z5.0 F12.8", "G0 X76.2 Y101.6 F800.0"]


def test_a_file_without_units_is_taken_to_be_in_machine_units(inch_machine):
    render(inch_machine, "G21\nG0 X1\n")
    render(inch_machine, "G0 X1 Y1\n")

    assert inch_machine.gcodeFileUnits == "INCHES"
