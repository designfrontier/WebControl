"""Gcode the motion actions queue for the controller: restarting mid-file, pause/resume, jogs and home."""
import pytest

from helpers import decode_ui_message, drain

SAFE_Z_MM = "G0 Z5.0 F12.8"
FEED = " F800.0"


@pytest.fixture
def job(data):
    """Saves and restores the run state a test sets up, leaving home at 0,0 and the machine in mm."""
    config = data.config
    keys = [
        ("Advanced Settings", "homeX"), ("Advanced Settings", "homeY"), ("Computed Settings", "units"),
        ("Computed Settings", "distToMove"), ("Computed Settings", "tolerance"),
    ]
    saved_config = [config.getValue(*key) for key in keys]
    attributes = [
        "gcode", "gcodeIndex", "gcodeShift", "zval", "currentZTarget", "positioningMode", "tolerance", "feedRate",
        "pausedzval", "pausedUnits", "pausedPositioningMode", "pausedGcodeIndex", "manualZAxisAdjust",
        "gcodeFileUnits",
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


def test_restart_converts_home_not_the_current_position_when_the_file_switches_units(job):
    job.config.setValue("Advanced Settings", "homeX", 25.4, recursionBreaker=True)
    job.config.setValue("Advanced Settings", "homeY", 50.8, recursionBreaker=True)
    load(job, ["G21", "G0 X10 Y10", "G20", "G0 X1 Y1", "G1 Z-0.1"], 5)

    job.actions.processGCode()

    assert drain(job.gcode_queue)[3] == "G0 X2.0 Y3.0" + FEED


def pause_at(data, lines, index, z_target):
    load(data, lines, index)
    data.uploadFlag = 1
    data.currentZTarget = z_target
    data.actions.pauseRun()
    drain(data.gcode_queue)
    drain(data.quick_queue)


def test_resume_returns_to_where_the_job_paused_before_lowering_the_bit(job):
    pause_at(job, METRIC_FILE + ["G1 X50 Y60"], 4, -3.0)
    job.actions.processAction({"data": {"command": "move", "arg": "right", "arg1": "100"}})
    drain(job.gcode_queue)

    job.actions.resumeRun()

    assert drain(job.gcode_queue) == [
        "G21 ", "G90 ", SAFE_Z_MM, "G0 X30.0 Y40.0" + FEED, "G0 Z-3.0 ", "G90 ",
    ]
    assert job.uploadFlag == 1
    assert drain(job.quick_queue) == ["~"]


def test_resume_goes_to_the_new_line_when_the_index_moved_while_paused(job):
    pause_at(job, METRIC_FILE + ["G0 Z2", "G0 X50 Y60", "G1 Z-1"], 4, -3.0)
    job.actions.processAction({"data": {"command": "moveGcodeIndex", "arg": "2", "arg1": ""}})
    drain(job.ui_queue1)

    job.actions.resumeRun()

    assert drain(job.gcode_queue) == [
        "G21 ", "G90 ", SAFE_Z_MM, "G0 X50.0 Y60.0" + FEED, "G0 Z2.0 ", "G90 ",
    ]


def test_resume_lowers_to_the_z_it_paused_at_when_the_file_has_not_moved_z_yet(job):
    pause_at(job, ["G90", "G0 X10 Y20", "G1 X30"], 2, 1.5)

    job.actions.resumeRun()

    assert drain(job.gcode_queue)[4] == "G0 Z1.5 "


def test_resume_restores_an_absolute_z_and_relative_mode_in_a_g91_file(job):
    pause_at(job, ["G91", "G0 X10 Y20", "G1 Z-3", "G1 X5"], 3, -3.0)

    job.actions.resumeRun()

    assert drain(job.gcode_queue) == [
        "G21 ", "G90 ", SAFE_Z_MM, "G0 X10.0 Y20.0" + FEED, "G0 Z-3.0 ", "G91 ",
    ]


def test_resume_restores_the_units_the_job_was_paused_in(job):
    pause_at(job, METRIC_FILE, 3, -3.0)
    job.actions.updateSetting("toInches", 1)
    drain(job.gcode_queue)

    job.actions.resumeRun()

    assert drain(job.gcode_queue)[:4] == ["G21 ", "G90 ", SAFE_Z_MM, "G0 X10.0 Y20.0" + FEED]
    assert job.units == "MM"


def test_resume_does_not_repeat_a_tool_change(job):
    pause_at(job, ["T2 M6"] + METRIC_FILE, 4, -3.0)

    job.actions.resumeRun()

    assert not any("M6" in line for line in drain(job.gcode_queue))


def test_return_to_center_converts_the_safe_height_to_inches(job):
    set_machine_units(job, "INCHES")

    job.actions.returnToCenter()

    assert drain(job.gcode_queue) == ["G90  ", "G00 Z0.197", "G00 X0.0 Y0.0 "]


@pytest.mark.parametrize(
    "line, xy_feed, z_feed",
    [("G1 Y10 F300", "F300.0", "F12.8"), ("G1 Z-1 F100", "F800.0", "F100.0"), ("G1 X10 F300", "F300.0", "F12.8")],
)
def test_restart_takes_the_xy_feed_from_any_xy_move_and_the_z_feed_from_z_moves(job, line, xy_feed, z_feed):
    load(job, ["G90", line, "G1 X1"], 2)

    job.actions.processGCode()

    queued = drain(job.gcode_queue)
    assert queued[2].endswith(z_feed)
    assert queued[3].endswith(xy_feed)


@pytest.mark.parametrize("word", ["G40", "G41 D1", "G43 H1"])
def test_restart_does_not_mistake_other_g4x_words_for_a_dwell(job, word):
    load(job, ["G90", word, "G1 X1"], 2)

    job.actions.processGCode()

    assert not any(line.startswith("G4") for line in drain(job.gcode_queue))


@pytest.mark.parametrize("word", ["G4 P2", "G04 P2"])
def test_restart_repeats_the_last_dwell(job, word):
    load(job, ["G90", word, "G1 X1"], 2)

    job.actions.processGCode()

    assert "G4 P2" in drain(job.gcode_queue)


@pytest.mark.parametrize(
    "command, alert",
    [
        ("setMaxZ", "Error with setting Z-Axis Maximum"),
        ("setMinZ", "Error with setting Z-Axis Minimum"),
        ("clearZ", "Error with clearing Z-Axis Min and Max"),
        ("getZlimits", "Error with clearing Z-Axis Min and Max"),
    ],
)
def test_z_limit_failures_raise_an_alert(job, monkeypatch, command, alert):
    job.uploadFlag = 0
    drain(job.ui_queue1)
    monkeypatch.setattr(job.actions, command, lambda: False)

    job.actions.processAction({"data": {"command": command, "arg": "", "arg1": ""}})

    assert [decode_ui_message(raw) for raw in drain(job.ui_queue1)] == [("Alert", "Alert", alert)]


@pytest.mark.parametrize("word", ["M30", "M300"])
def test_restart_does_not_mistake_longer_m_codes_for_spindle_on(job, word):
    load(job, ["G90", "G0 X1", word, "G1 X2"], 3)

    job.actions.processGCode()

    assert "M3 " not in drain(job.gcode_queue)


@pytest.mark.parametrize("word, spindle", [("M04", "M4 "), ("M4", "M4 "), ("M05", "M5 "), ("M03 S1000", "M3 ")])
def test_restart_restores_the_spindle_direction(job, word, spindle):
    load(job, ["G90", word, "G1 X2"], 2)

    job.actions.processGCode()

    assert spindle in drain(job.gcode_queue)


def test_restart_does_not_mistake_g91_1_arc_centres_for_relative_moves(job):
    load(job, ["G90", "G91.1", "G0 X10 Y20", "G0 X30 Y40", "G1 Z-1"], 5)

    job.actions.processGCode()

    queued = drain(job.gcode_queue)
    assert queued[3] == "G0 X30.0 Y40.0" + FEED
    assert queued[-1] == "G90 "
