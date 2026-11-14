"""The classic gcode form routes refuse to replace the loaded gcode while a job is running or paused."""
import io
import os

import pytest

MESSAGE = "Cannot load gcode while a job is running or paused. Stop the job first."


@pytest.fixture
def running_job(data):
    saved = (data.gcode, data.gcodeFile.filename, data.manualZAxisAdjust, data.uploadFlag)
    data.gcode = ["G0 X1 ", "G1 Z-1 "]
    yield data
    data.gcode, data.gcodeFile.filename, data.manualZAxisAdjust, data.uploadFlag = saved


def post(client, path):
    forms = {
        "/openGCode": {"selectedGCode": "./other.nc"},
        "/uploadGCode": {"selectedDirectory": ".", "file[]": (io.BytesIO(b"G0 X9\n"), "guarded-upload.nc")},
        "/editGCode": {"gcode": "G0 X9"},
    }
    return client.post(path, data=forms[path], content_type="multipart/form-data")


@pytest.mark.parametrize("path", ["/openGCode", "/uploadGCode", "/editGCode"])
@pytest.mark.parametrize("flag", [1, 2, -1])
def test_gcode_cannot_be_replaced_during_a_job(client, running_job, path, flag):
    running_job.uploadFlag = flag

    response = post(client, path)

    assert response.status_code == 409
    assert response.status == "409 " + MESSAGE
    assert response.get_json() == {"status": 409, "error": MESSAGE}
    assert running_job.gcode == ["G0 X1 ", "G1 Z-1 "]


def test_an_upload_during_a_job_is_not_saved(client, running_job):
    running_job.uploadFlag = 1

    post(client, "/uploadGCode")

    home = running_job.config.getHome()
    assert not os.path.exists(os.path.join(home, ".WebControl", "gcode", "guarded-upload.nc"))


def test_gcode_cannot_be_replaced_while_waiting_for_a_tool_change(client, running_job):
    running_job.uploadFlag = 0
    running_job.manualZAxisAdjust = True

    assert post(client, "/editGCode").status_code == 409


def test_gcode_can_be_edited_when_no_job_is_running(client, running_job):
    running_job.uploadFlag = 0

    assert post(client, "/editGCode").status_code == 200
