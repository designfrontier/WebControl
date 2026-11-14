import pytest

from api.calibration import OPTICAL, optical_settings
from helpers import NAMESPACE

TRIANGULAR = {"cut12": 1930, "cut34": "1930", "cut5": 300, "bitDiameter": 6.35}

OPTICAL_SETTINGS = {
    "markerX": 0.75,
    "markerY": "0.75",
    "scaleX": 1.002,
    "scaleY": 1.003,
    "opticalCenterX": 320.5,
    "opticalCenterY": 240.25,
    "positionTolerance": 0.2,
    "tlX": -15,
    "tlY": 7,
    "brX": 0,
    "brY": 0,
    "calibrationExtents": "Top Left",
    "autoScanDirection": "1",
    "gaussianBlurValue": 7,
    "cannyLowValue": 40,
    "cannyHighValue": 120,
}


@pytest.fixture
def restore_optical(data):
    saved = optical_settings(data)
    yield
    data.opticalCalibration.saveOpticalCalibrationConfiguration(saved)


def test_triangular_reports_the_current_parameters(client, data):
    body = client.get("/api/calibration/triangular").get_json()
    assert body == {
        "motorYoffset": float(data.config.getValue("Maslow Settings", "motorOffsetY")),
        "rotationRadius": float(data.config.getValue("Advanced Settings", "rotationRadius")),
        "chainSagCorrection": float(data.config.getValue("Advanced Settings", "chainSagCorrection")),
    }


def test_triangular_calculates_without_saving(client, data):
    before = data.config.getValue("Maslow Settings", "motorOffsetY")
    response = client.post("/api/calibration/triangular", json=TRIANGULAR)
    assert response.status_code == 200
    body = response.get_json()
    assert set(body) == {"motorYoffset", "rotationRadius", "chainSagCorrection", "calibrationError"}
    assert body["motorYoffset"] == pytest.approx(400.6, abs=5)
    assert data.config.getValue("Maslow Settings", "motorOffsetY") == before
    assert data.triangularCalibration.motorYoffsetEst == body["motorYoffset"]


@pytest.mark.parametrize(
    "change, message",
    [
        ({"cut12": "abc"}, "Please enter a number for the distance between cut 1 and cut 2."),
        ({"cut34": None}, "Please enter a number for the distance between cut 3 and cut 4."),
        ({"cut5": True}, "Please enter a number for the distance between the top of the work area and cut 5."),
        ({"bitDiameter": "nan"}, "Please enter a number for the bit diameter."),
        ({"cut12": 3000}, "The measurement between cut 1 and cut 2 of 3000.0 mm seems wrong."),
        ({"cut34": 100}, "The measurement between cut 3 and cut 4 of 100.0 mm seems wrong."),
        ({"cut5": -1}, "The measurement between the top edge of the work area and cut 5 of -1.0 mm seems wrong."),
        ({"bitDiameter": 30}, "The bit diameter of 30.0 mm seems wrong."),
    ],
)
def test_triangular_rejects_bad_measurements(client, data, change, message):
    response = client.post("/api/calibration/triangular", json={**TRIANGULAR, **change})
    assert response.status_code == 400
    assert response.get_json()["error"].startswith(message)
    assert data.message_queue.empty()


def test_triangular_treats_a_non_object_body_as_empty(client, data):
    response = client.post("/api/calibration/triangular", json=[1, 2])
    assert response.status_code == 400


def test_triangular_reports_when_it_cannot_converge(client, data, monkeypatch):
    monkeypatch.setattr(data.actions, "calibrate", lambda values: False)
    response = client.post("/api/calibration/triangular", json=TRIANGULAR)
    assert response.status_code == 422
    assert "could not be calibrated" in response.get_json()["error"]


def test_holey_reports_the_current_parameters(client, data):
    body = client.get("/api/calibration/holey").get_json()
    assert body["distanceBetweenMotors"] == float(data.config.getValue("Maslow Settings", "motorSpacingX"))
    assert set(body) == {"motorYoffset", "distanceBetweenMotors", "leftChainTolerance", "rightChainTolerance"}


def test_holey_calculates_from_twelve_measurements(client, data):
    ideal = [float(value) for value in data.holeyCalibration.InitializeIdealXyCoordinates()]
    response = client.post("/api/calibration/holey", json={"measurements": [str(ideal[0]), *ideal[1:]]})
    assert response.status_code == 200
    body = response.get_json()
    assert set(body) == {
        "motorYoffset",
        "distanceBetweenMotors",
        "leftChainTolerance",
        "rightChainTolerance",
        "calibrationError",
    }
    assert all(isinstance(value, float) for value in body.values())


@pytest.mark.parametrize(
    "body, message",
    [
        ({}, "Please enter all 12 measurements."),
        ({"measurements": [1] * 11}, "Please enter all 12 measurements."),
        ({"measurements": [1] * 11 + ["x"]}, "Please enter a number for the distance M12."),
    ],
)
def test_holey_rejects_bad_measurements(client, data, body, message):
    response = client.post("/api/calibration/holey", json=body)
    assert response.status_code == 400
    assert response.get_json() == {"error": message}


def test_holey_reports_a_failed_fit(client, data, monkeypatch):
    monkeypatch.setattr(data.actions, "holeyCalibrate", lambda values: False)
    response = client.post("/api/calibration/holey", json={"measurements": [1] * 12})
    assert response.status_code == 422


def test_optical_serves_settings_curve_and_error_matrix(client, data):
    body = client.get("/api/calibration/optical").get_json()
    assert body["settings"]["calibrationExtents"] == data.config.getValue(OPTICAL, "calibrationExtents")
    assert isinstance(body["settings"]["tlX"], int)
    assert body["curve"] == {"curveX": [0.0] * 6, "curveY": [0.0] * 6}
    assert len(body["error"]["errorX"]) == 31
    assert all(len(column) == 15 for column in body["error"]["errorX"])


def test_optical_falls_back_to_defaults_for_missing_settings(data, monkeypatch):
    monkeypatch.setattr(data.config, "getValue", lambda section, key: None)
    settings = optical_settings(data)
    assert settings["calibrationExtents"] == "Full Area"
    assert settings["gaussianBlurValue"] == 5
    assert settings["scaleX"] == 1.0


def test_optical_error_matrix_is_empty_when_unset(client, data, monkeypatch):
    real = data.config.getValue
    monkeypatch.setattr(data.config, "getValue", lambda s, k: "" if k == "xyErrorArray" else real(s, k))
    assert client.get("/api/calibration/optical").get_json()["error"] == {"errorX": [], "errorY": []}


def test_optical_settings_save_and_read_back(client, data, restore_optical):
    response = client.put("/api/calibration/optical/settings", json=OPTICAL_SETTINGS)
    assert response.status_code == 200
    settings = response.get_json()["settings"]
    assert settings["opticalCenterX"] == 320.5
    assert settings["autoScanDirection"] == 1
    assert settings["calibrationExtents"] == "Top Left"
    assert client.get("/api/calibration/optical").get_json()["settings"] == settings


@pytest.mark.parametrize(
    "body, message",
    [
        (None, "Expected the optical calibration settings."),
        ({"markerX": 1}, "Missing markerY"),
        ({**OPTICAL_SETTINGS, "calibrationExtents": 3}, "Please choose the calibration extents."),
        ({**OPTICAL_SETTINGS, "scaleX": "big"}, "Please enter a number for scaleX."),
    ],
)
def test_optical_settings_reject_bad_input(client, data, body, message):
    response = client.put("/api/calibration/optical/settings", json=body)
    assert response.status_code == 400
    assert response.get_json()["error"].startswith(message)


def test_optical_settings_report_a_failed_save(client, data, monkeypatch):
    monkeypatch.setattr(data.opticalCalibration, "saveOpticalCalibrationConfiguration", lambda values: False)
    response = client.put("/api/calibration/optical/settings", json=OPTICAL_SETTINGS)
    assert response.status_code == 500


@pytest.fixture
def processed(data, monkeypatch):
    calls = []
    monkeypatch.setattr(data.actions, "processAction", calls.append)
    return calls


@pytest.mark.parametrize("command", ["optical_Calibrate", "testOpticalCalibration", "findCenterOpticalCalibration"])
def test_optical_event_runs_the_action_with_parsed_settings(socket_client, processed, command):
    socket_client.emit("opticalCalibration", {"data": {"command": command, "arg": OPTICAL_SETTINGS}}, namespace=NAMESPACE)
    assert len(processed) == 1
    arg = processed[0]["data"]["arg"]
    assert processed[0]["data"]["command"] == command
    assert arg["markerY"] == 0.75
    assert arg["autoScanDirection"] == 1


@pytest.mark.parametrize(
    "payload",
    [
        {"data": {"command": "stopRun", "arg": OPTICAL_SETTINGS}},
        {"data": {"command": "testOpticalCalibration", "arg": "x"}},
        {"data": "Please"},
        "nonsense",
    ],
)
def test_optical_event_ignores_other_commands_and_bad_settings(socket_client, data, processed, payload):
    socket_client.emit("opticalCalibration", payload, namespace=NAMESPACE)
    assert processed == []


def test_optical_event_names_an_unknown_command(socket_client, data, processed):
    socket_client.emit("opticalCalibration", {"data": {"command": "stopRun", "arg": OPTICAL_SETTINGS}}, namespace=NAMESPACE)
    logged = [data.console_queue.get_nowait() for _ in range(data.console_queue.qsize())]
    assert "Ignoring opticalCalibration request: unknown command 'stopRun'" in logged
    assert processed == []
