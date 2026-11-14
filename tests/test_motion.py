"""Gcode the motion actions queue for the controller: restarting mid-file, pause/resume, jogs and home."""
import pytest

from helpers import drain

SAFE_Z_MM = "G0 Z5.0 F12.8"
FEED = " F800.0"


@pytest.fixture
def job(data):
    """Saves and restores the run state a test sets up, leaving home at 0,0 and the machine in mm."""
    config = data.config
    keys = [("Advanced Settings", "homeX"), ("Advanced Settings", "homeY"), ("Computed Settings", "units")]
    saved_config = [config.getValue(*key) for key in keys]
    attributes = [
        "gcode", "gcodeIndex", "gcodeShift", "zval", "currentZTarget", "positioningMode",
        "pausedzval", "pausedUnits", "pausedPositioningMode", "manualZAxisAdjust", "gcodeFileUnits",
    ]
    saved = {name: getattr(data, name) for name in attributes}
    set_machine_units(data, "MM")
    config.setValue("Advanced Settings", "homeX", 0.0, recursionBreaker=True)
    config.setValue("Advanced Settings", "homeY", 0.0, recursionBreaker=True)
    data.gcodeShift = [0.0, 0.0]
    data.positioningMode = 0
    data.manualZAxisAdjust = False
    yield data
    for key, value in zip(keys, saved_config):
        config.setValue(*key, value, recursionBreaker=True)
    for name, value in saved.items():
        setattr(data, name, value)
    drain(data.gcode_queue)


def set_machine_units(data, units):
    data.units = units
    data.config.setValue("Computed Settings", "units", units, recursionBreaker=True)


def load(data, lines, index):
    data.gcode = [line + " " for line in lines]
    data.gcodeIndex = index


METRIC_FILE = ["G90", "G0 X10 Y20", "G1 Z-3", "G1 X30 Y40"]


def test_restart_mid_file_keeps_machine_mm_when_file_has_no_units(job):
    load(job, METRIC_FILE, 3)

    job.actions.processGCode()

    assert drain(job.gcode_queue) == [
        "G21 ", "G90 ", SAFE_Z_MM, "G0 X10.0 Y20.0" + FEED, "G0 Z-3.0 ", "G90 ",
    ]


def test_restart_mid_file_keeps_machine_inches_when_file_has_no_units(job):
    set_machine_units(job, "INCHES")
    load(job, METRIC_FILE, 3)

    job.actions.processGCode()

    assert drain(job.gcode_queue)[:2] == ["G20 ", "G90 "]


def test_restart_mid_file_uses_the_files_units_over_the_machines(job):
    load(job, ["G20"] + METRIC_FILE, 4)

    job.actions.processGCode()

    assert drain(job.gcode_queue)[:3] == ["G20 ", "G90 ", "G0 Z0.1969 F12.8"]
