"""A job the controller paused (M0/M1, Message:, ALARM) behaves like a user Pause: jog, Play resumes, home is locked."""
import pytest

from helpers import decode_ui_message, drain
from test_job_in_progress import action, alarm, from_controller, job, start  # noqa: F401
from test_serial_streaming import serial  # noqa: F401


def program_pause(app, data, serial):  # noqa: F811
    serial.thread().managePause("M0 ")
    from_controller(app, data, "Maslow Paused\r\n")


PAUSES = [program_pause, alarm]
PID_BODY = {"kind": "velocity", "motor": "L", "start": 0, "stop": 1, "steps": 1, "version": 1, "kp": 1, "ki": 1, "kd": 1}


@pytest.fixture
def paused(app, job, serial, request):  # noqa: F811
    saved = (job.firmwarePaused, job.config.getValue("Advanced Settings", "homeX"), job.firmwareHoldsJobLines)
    start(job)
    job.gcodeIndex = 5
    request.param(app, job, serial)
    assert (job.uploadFlag, job.jobInProgress) == (0, True)
    [drain(queue) for queue in (job.ui_queue1, job.gcode_queue, job.quick_queue)]
    yield job
    job.firmwarePaused = saved[0]
    job.firmwareHoldsJobLines = saved[2]
    job.config.setValue("Advanced Settings", "homeX", saved[1], recursionBreaker=True)


def alerts(data):
    return [text for command, _, text in map(decode_ui_message, drain(data.ui_queue1)) if command == "Alert"]


@pytest.mark.parametrize("paused", PAUSES, indirect=True)
def test_jogging_is_allowed(paused):
    paused.actions.processAction({"data": {"command": "move", "arg": "right", "arg1": "10"}})

    assert alerts(paused) == []
    assert drain(paused.gcode_queue) != []


@pytest.mark.parametrize("paused", PAUSES, indirect=True)
def test_play_resumes_the_job_and_releases_the_firmware(paused):
    action(paused, "startRun")

    assert alerts(paused) == []
    assert drain(paused.quick_queue) == ["~"]
    assert (paused.uploadFlag, paused.jobInProgress, paused.gcodeIndex) == (1, True, 5)


@pytest.mark.parametrize("paused", PAUSES, indirect=True)
def test_play_while_the_job_streams_or_is_user_paused_is_refused(paused):
    action(paused, "startRun")
    action(paused, "pauseRun")
    drain(paused.ui_queue1)

    action(paused, "startRun")

    assert paused.uploadFlag == 2
    assert alerts(paused) == [
        "Cannot issue command while paused sending gcode. You must press STOP before performing this action."
    ]


@pytest.mark.parametrize("paused", PAUSES, indirect=True)
@pytest.mark.parametrize("command", ["defineHome", "resetHomeToCenter"])
def test_home_cannot_be_moved_until_the_job_is_stopped(paused, command):
    home = paused.config.getValue("Advanced Settings", "homeX")

    paused.actions.processAction({"data": {"command": command, "arg": 12, "arg1": 3}})

    assert paused.config.getValue("Advanced Settings", "homeX") == home
    assert alerts(paused) == [
        "Cannot issue command while paused sending gcode. You must press STOP before performing this action."
    ]


@pytest.mark.parametrize("paused", PAUSES, indirect=True)
def test_a_pid_test_cannot_start_until_the_job_is_stopped(client, paused):
    response = client.post("/api/system/pid-test", json=PID_BODY)

    assert response.status_code == 409
    assert response.get_json() == {"error": "Stop the job before starting a PID test."}
    assert drain(paused.gcode_queue) == []


@pytest.mark.parametrize("paused", PAUSES, indirect=True)
def test_a_reloaded_page_offers_resume(client, paused):
    assert client.get("/api/state").get_json()["pauseButtonSetting"] == "Resume"


@pytest.mark.parametrize("paused", PAUSES, indirect=True)
def test_home_can_be_set_again_once_the_job_is_stopped(paused):
    action(paused, "stopRun")
    drain(paused.ui_queue1)

    paused.actions.processAction({"data": {"command": "defineHome", "arg": 1, "arg1": 0}})

    assert alerts(paused) == []


def test_home_cannot_be_moved_while_a_tool_change_waits_outside_a_job(job):
    saved = job.config.getValue("Advanced Settings", "homeX")
    job.uploadFlag, job.jobInProgress, job.manualZAxisAdjust = 0, False, True

    job.actions.processAction({"data": {"command": "defineHome", "arg": 12, "arg1": 3}})

    assert job.config.getValue("Advanced Settings", "homeX") == saved
    assert alerts(job) == [
        "Cannot issue command while a tool change waits. Press Resume or STOP before setting home or running a PID test."
    ]


@pytest.mark.parametrize("paused", PAUSES, indirect=True)
def test_a_reloaded_classic_page_offers_resume(paused):
    assert paused.actions.processSettingRequest("None", "pauseButtonSetting") == ("pauseButtonSetting", "Resume")
