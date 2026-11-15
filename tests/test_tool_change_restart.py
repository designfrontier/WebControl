"""Play after a tool change or a stop must never lower the bit to a Z the file has not set for the current tool."""
from helpers import drain
from test_motion import SAFE_Z_MM, job, load  # noqa: F401

TOOL_CHANGE_FILE = [
    "G21", "G90", "G0 Z5", "G0 X10 Y10", "G1 Z-3", "G1 X20", "G0 Z5", "G0 X0 Y0",
    "T2 M6", "G0 X40 Y40", "G0 Z5", "G1 Z-2", "G1 X50",
]
AFTER_TOOL_CHANGE = TOOL_CHANGE_FILE.index("T2 M6") + 1


def lowered_to(queued):
    return [line for line in queued if line.startswith(("G0 Z", "G1 Z")) and line != SAFE_Z_MM]


def test_a_restart_before_the_file_sets_z_stays_at_safe_height(job):
    load(job, ["G21", "G90", "G0 X10 Y10", "G1 X20 Y20"], 3)

    job.actions.processGCode()

    assert lowered_to(drain(job.gcode_queue)) == []


def test_a_restart_after_a_tool_change_stays_at_safe_height(job):
    load(job, TOOL_CHANGE_FILE, AFTER_TOOL_CHANGE)

    job.actions.processGCode()

    assert lowered_to(drain(job.gcode_queue)) == []


def test_a_restart_once_the_new_tool_has_a_z_lowers_to_it(job, monkeypatch):
    monkeypatch.setattr(job, "currentTool", 2)
    load(job, TOOL_CHANGE_FILE, len(TOOL_CHANGE_FILE) - 1)

    job.actions.processGCode()

    assert lowered_to(drain(job.gcode_queue)) == ["G1 Z-2.0 F12.8 "]


def test_play_during_a_tool_change_resumes_where_it_paused(job):
    load(job, TOOL_CHANGE_FILE, AFTER_TOOL_CHANGE)
    job.uploadFlag = 0
    job.manualZAxisAdjust = True
    job.toolChangeWaits = True
    job.jobInProgress = True

    job.actions.processAction({"data": {"command": "startRun", "arg": "", "arg1": ""}})

    queued = drain(job.gcode_queue)
    assert queued[2:4] == [SAFE_Z_MM, "G0 X0.0 Y0.0 "]
    assert lowered_to(queued) == []
    assert (job.uploadFlag, job.manualZAxisAdjust, job.toolChangeWaits) == (1, False, False)
    assert job.gcodeIndex == AFTER_TOOL_CHANGE
