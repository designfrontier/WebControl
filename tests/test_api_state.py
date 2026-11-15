from types import SimpleNamespace

import pytest

from api.state import startup_state

SETTINGS = {
    ("Computed Settings", "units"): "INCHES",
    ("Computed Settings", "distToMove"): "2.5",
    ("Computed Settings", "unitsZ"): "MM",
    ("Computed Settings", "distToMoveZ"): 1,
    ("Advanced Settings", "homeX"): "12.5",
    ("Advanced Settings", "homeY"): "-3",
    ("Maslow Settings", "macro1_title"): "Spindle on",
    ("Maslow Settings", "macro2_title"): "Dust collector",
}


def fake_data(upload_flag=0, overrides=None, firmware=51.27, job=False, manual_z=False):
    settings = {**SETTINGS, **(overrides or {})}
    config = SimpleNamespace(getValue=lambda section, key: settings.get((section, key)))
    return SimpleNamespace(
        config=config,
        uploadFlag=upload_flag,
        hostAddress="10.0.0.5:5000",
        controllerFirmwareVersion=firmware,
        jobInProgress=job,
        manualZAxisAdjust=manual_z,
        controllerBoard="PCB v1.6 TLE9201  Detected",
        firmwareRewriteNoticeSeen=False,
    )


def test_collects_the_computed_settings_the_ui_needs_at_startup():
    assert startup_state(fake_data()) == {
        "units": "INCHES",
        "distToMove": 2.5,
        "unitsZ": "MM",
        "distToMoveZ": 1.0,
        "homePosition": {"x": 12.5, "y": -3.0},
        "pauseButtonSetting": "Pause",
        "jobInProgress": False,
        "manualZAxisAdjust": False,
        "hostAddress": "10.0.0.5:5000",
        "controllerFirmwareVersion": 51.27,
        "controllerBoard": "PCB v1.6 TLE9201  Detected",
        "firmwareBuild": "51.27",
        "macros": [{"command": "macro1", "title": "Spindle on"}, {"command": "macro2", "title": "Dust collector"}],
    }


@pytest.mark.parametrize("upload_flag, label", [(0, "Pause"), (1, "Pause"), (2, "Resume"), (-1, "Resume")])
def test_pause_button_matches_the_upload_flag(upload_flag, label):
    assert startup_state(fake_data(upload_flag))["pauseButtonSetting"] == label


def test_unparseable_or_missing_values_fall_back():
    state = startup_state(fake_data(0, {("Computed Settings", "distToMove"): "abc", ("Advanced Settings", "homeX"): None}))
    assert state["distToMove"] == 0.0
    assert state["homePosition"]["x"] == 0.0


@pytest.mark.parametrize("job, manual_z", [(True, False), (True, True), (False, False)])
def test_reports_whether_a_job_is_in_progress_and_waiting_on_a_tool_change(job, manual_z):
    state = startup_state(fake_data(job=job, manual_z=manual_z))
    assert (state["jobInProgress"], state["manualZAxisAdjust"]) == (job, manual_z)


def test_a_job_paused_by_the_controller_is_still_in_progress(client, data, monkeypatch):
    monkeypatch.setattr(data, "uploadFlag", 0)
    monkeypatch.setattr(data, "jobInProgress", True)
    assert client.get("/api/state").get_json()["jobInProgress"] is True


def test_firmware_version_is_zero_until_the_controller_reports_one():
    assert startup_state(fake_data(firmware=0))["controllerFirmwareVersion"] == 0.0
    assert startup_state(fake_data(firmware=None))["controllerFirmwareVersion"] == 0.0


def test_the_board_and_build_are_null_until_the_controller_reports_them():
    data = fake_data(firmware=0)
    data.controllerBoard = None
    assert (startup_state(data)["controllerBoard"], startup_state(data)["firmwareBuild"]) == (None, None)


def test_missing_macro_title_is_empty():
    state = startup_state(fake_data(0, {("Maslow Settings", "macro2_title"): None}))
    assert state["macros"][1] == {"command": "macro2", "title": ""}


def test_endpoint_serves_the_live_config(client, data):
    data.uploadFlag = 0
    body = client.get("/api/state").get_json()
    assert body["units"] == data.config.getValue("Computed Settings", "units")
    assert body["hostAddress"] == data.hostAddress
    assert body["pauseButtonSetting"] == "Pause"
    assert [macro["command"] for macro in body["macros"]] == ["macro1", "macro2"]
    assert body["controllerFirmwareVersion"] == float(data.controllerFirmwareVersion)
