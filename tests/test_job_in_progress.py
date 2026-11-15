"""A job stays in progress through every kind of pause, so its gcode cannot be replaced until Stop or the file ends."""
import io

import pytest

from Background.messageProcessor import MessageProcessor
from Background.UIProcessor import UIProcessor
from helpers import drain
from test_serial_streaming import once_after, serial  # noqa: F401

LOCKED = "Cannot load gcode while a job is running or paused. Stop the job first."
JOB = [f"G1 X{n} " for n in range(20)]
SAVED = [
    "jobInProgress", "manualZAxisAdjust", "previousUploadStatus", "pausedGcodeIndex", "pausedzval",
    "pausedUnits", "pausedPositioningMode",
]


@pytest.fixture
def job(app, data, serial):  # noqa: F811
    saved = {key: getattr(data, key) for key in SAVED}
    saved_filename = data.gcodeFile.filename
    saved_first_run = data.config.firstRun
    data.config.firstRun = False
    yield data
    for key, value in saved.items():
        setattr(data, key, value)
    data.gcodeFile.filename = saved_filename
    data.config.firstRun = saved_first_run
    [drain(queue) for queue in [data.ui_queue1, data.ui_controller_queue, data.message_queue, data.quick_queue]]


def action(data, command):
    data.actions.processAction({"data": {"command": command, "arg": "", "arg1": ""}})


def start(data):
    data.gcode = list(JOB)
    data.gcodeIndex = 0
    action(data, "startRun")
    assert data.uploadFlag == 1


def from_controller(app, data, message):
    processor = MessageProcessor()
    processor.setUpData(data)
    processor.processMessage(message)
    ui = UIProcessor()
    ui.app = app
    with app.app_context():
        ui.processPass()


def user_pause(app, data, serial):  # noqa: F811
    action(data, "pauseRun")


def m0_pause(app, data, serial):  # noqa: F811
    serial.thread().managePause("M0 ")


def tool_change(app, data, serial):  # noqa: F811
    serial.thread().manageToolChange("T7 M6 ")
    from_controller(app, data, "Tool Change: Please insert tool 7\r\n")


def controller_pause(app, data, serial):  # noqa: F811
    from_controller(app, data, "Maslow Paused\r\n")


def adjust_z_message(app, data, serial):  # noqa: F811
    from_controller(app, data, "Message: Please adjust Z-Axis to a depth of -3.00 mm\r\n")


def alarm(app, data, serial):  # noqa: F811
    from_controller(app, data, "ALARM: The sled is not keeping up with its expected position and has halted.\r\n")


def buffer_overflow(app, data, serial):  # noqa: F811
    from_controller(app, data, "Buffer overflow!\r\n")


def connection_loss(app, data, serial):  # noqa: F811
    serial.run(on_poll=once_after(serial.clock, 1, lambda port: setattr(port, "ack", False)))


PAUSES = [user_pause, m0_pause, tool_change, controller_pause, adjust_z_message, alarm, buffer_overflow, connection_loss]


def post(client, path):
    forms = {
        "/openGCode": {"selectedGCode": "./other.nc"},
        "/uploadGCode": {"selectedDirectory": ".", "file[]": (io.BytesIO(b"G0 X9\n"), "paused-upload.nc")},
        "/editGCode": {"gcode": "G0 X9"},
    }
    return client.post(path, data=forms[path], content_type="multipart/form-data")


def test_starting_a_run_puts_a_job_in_progress(job):
    start(job)

    assert job.jobInProgress is True


@pytest.mark.parametrize("pause", PAUSES)
def test_every_kind_of_pause_keeps_the_job_in_progress(app, job, serial, pause):  # noqa: F811
    start(job)

    pause(app, job, serial)

    assert job.jobInProgress is True


@pytest.mark.parametrize("path", ["/openGCode", "/uploadGCode", "/editGCode"])
@pytest.mark.parametrize("pause", [controller_pause, adjust_z_message, alarm, buffer_overflow])
def test_gcode_cannot_be_loaded_after_the_controller_pauses_the_job(app, client, job, serial, pause, path):  # noqa: F811
    start(job)
    pause(app, job, serial)
    assert job.uploadFlag == 0

    response = post(client, path)

    assert response.status_code == 409
    assert response.status == "409 " + LOCKED
    assert response.get_json() == {"status": 409, "error": LOCKED}
    assert job.gcode == JOB


@pytest.mark.parametrize("pause", PAUSES)
def test_gcode_cannot_be_cleared_while_a_job_is_paused(app, job, serial, pause):  # noqa: F811
    start(job)
    pause(app, job, serial)
    drain(job.ui_queue1)

    action(job, "clearGCode")

    assert job.gcode == JOB
    assert any('"Alert"' in raw for raw in drain(job.ui_queue1))


def test_clearing_after_a_controller_pause_says_why_it_was_refused(app, job, serial):  # noqa: F811
    start(job)
    alarm(app, job, serial)
    drain(job.ui_queue1)

    action(job, "clearGCode")

    assert any(LOCKED in raw for raw in drain(job.ui_queue1))


def test_stop_ends_the_job(app, job, serial):  # noqa: F811
    start(job)
    alarm(app, job, serial)

    action(job, "stopRun")

    assert job.jobInProgress is False


def test_reaching_the_end_of_the_file_ends_the_job(job, serial):  # noqa: F811
    start(job)
    job.gcode = ["G1 X1 ", "G1 X2 "]

    serial.run(on_poll=once_after(serial.clock, 0, lambda port: drain(job.gcode_queue)))

    assert job.uploadFlag == 0
    assert job.jobInProgress is False


def test_loading_gcode_while_idle_leaves_no_job_in_progress(client, job):
    job.gcode = list(JOB)
    job.jobInProgress = False

    assert post(client, "/editGCode").status_code == 200
    assert job.jobInProgress is False


def test_a_finished_job_lets_the_next_file_load_and_run(client, job, serial):  # noqa: F811
    start(job)
    job.gcode = ["G1 X1 "]
    serial.run(on_poll=once_after(serial.clock, 0, lambda port: drain(job.gcode_queue)))

    assert post(client, "/editGCode").status_code == 200
    assert job.gcode == ["G0 X9 "]
    action(job, "startRun")
    assert (job.uploadFlag, job.jobInProgress) == (1, True)


def test_clearing_gcode_while_idle_still_works(job):
    job.gcode = list(JOB)
    job.jobInProgress = False

    action(job, "clearGCode")

    assert job.gcode == []


def test_a_start_that_fails_part_way_leaves_no_job_in_progress(job, monkeypatch):
    def broken_led(*args):
        raise RuntimeError("gpio unavailable")

    monkeypatch.setattr(job.gpioActions, "causeAction", broken_led)
    job.gcode = list(JOB)
    job.gcodeIndex = 0

    action(job, "startRun")

    assert (job.uploadFlag, job.jobInProgress, job.gcodeIndex) == (0, False, 0)
