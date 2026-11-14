from types import SimpleNamespace

import pytest

from api.main import main_screen_state


def fake_data(touch_plate=0, gcode=None, gcode_index=0, upload_flag=0):
    settings = {("Advanced Settings", "touchPlate"): touch_plate}
    return SimpleNamespace(
        config=SimpleNamespace(getValue=lambda section, key: settings.get((section, key))),
        uploadFlag=upload_flag,
        positioningMode=1,
        currentTool=3,
        gcode=gcode if gcode is not None else [],
        gcodeIndex=gcode_index,
    )


def test_reports_the_run_status_the_controls_are_gated_on():
    assert main_screen_state(fake_data(upload_flag=2))["status"] == {
        "uploadFlag": 2,
        "positioningMode": 1,
        "currentTool": 3,
    }


@pytest.mark.parametrize(
    "stored, enabled",
    [
        (0, False),
        (1, True),
        ("0", False),
        ("1", True),
        ("1.0", True),
        (True, True),
        ("true", True),
        (None, False),
        ("", False),
    ],
)
def test_touch_plate_flag_accepts_every_stored_form(stored, enabled):
    assert main_screen_state(fake_data(touch_plate=stored))["touchPlate"] is enabled


def test_reports_the_current_gcode_line():
    state = main_screen_state(fake_data(gcode=["G21", "G0 X1", "G1 Z-1"], gcode_index=1))
    assert state["gcode"] == {"lineIndex": 1, "line": "G0 X1", "lineCount": 3}


def test_no_gcode_loaded():
    assert main_screen_state(fake_data())["gcode"] == {"lineIndex": 0, "line": "", "lineCount": 0}


def test_out_of_range_index_falls_back_to_the_first_line():
    assert main_screen_state(fake_data(gcode=["G21"], gcode_index=5))["gcode"]["line"] == "G21"


def test_endpoint_serves_live_data(client, data):
    data.uploadFlag = 1
    body = client.get("/api/main").get_json()
    assert body["status"]["uploadFlag"] == 1
    assert body["status"]["currentTool"] == data.currentTool
    assert isinstance(body["touchPlate"], bool)
    assert body["gcode"]["lineCount"] == len(data.gcode)
