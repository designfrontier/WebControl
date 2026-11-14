import pytest

from calibration_helpers import calibration, holey_form, ideal_holey_measurements  # noqa: F401

TRIANGULAR = {"cut12": "1930", "cut34": "1930", "cut5": "300", "bitDiameter": "6.35"}
ZERO_TRIANGULAR = (0.0, 138.8, 33.4, -0.13)
ZERO_HOLEY = (0.0, 2978.4, 0.0, 0.0, 0.0)


def raise_error(*args):
    raise ValueError("boom")


def test_calibrate_returns_none_for_bad_measurements(calibration):
    assert calibration.actions.calibrate({**TRIANGULAR, "cut12": "x"}) is None
    assert calibration.triangularCalibration.calculate({**TRIANGULAR, "cut12": "x"}) is None


def test_calibrate_returns_none_when_the_calculation_raises(calibration, monkeypatch):
    monkeypatch.setattr(calibration.triangularCalibration, "calculate", raise_error)
    assert calibration.actions.calibrate(TRIANGULAR) is None


def test_calibrate_keeps_a_zero_motor_offset(calibration, monkeypatch):
    monkeypatch.setattr(calibration.triangularCalibration, "calculate", lambda result: ZERO_TRIANGULAR)
    assert calibration.actions.calibrate(TRIANGULAR) == ZERO_TRIANGULAR


def test_holey_calibrate_returns_none_for_a_missing_measurement(calibration):
    measurements = ideal_holey_measurements(calibration)
    assert calibration.actions.holeyCalibrate(holey_form(measurements[:-1] + [""])) is None


def test_holey_calibrate_returns_none_when_the_fit_raises(calibration, monkeypatch):
    monkeypatch.setattr(calibration.holeyCalibration, "Calibrate", raise_error)
    assert calibration.actions.holeyCalibrate({}) is None


def test_holey_calibrate_keeps_a_zero_motor_offset(calibration, monkeypatch):
    monkeypatch.setattr(calibration.holeyCalibration, "Calibrate", lambda result: ZERO_HOLEY)
    assert calibration.actions.holeyCalibrate({}) == ZERO_HOLEY


def test_triangular_route_answers_json_when_the_calculation_fails(client, calibration):
    response = client.post("/triangularCalibration", data={**TRIANGULAR, "cut12": "x"})
    assert response.status_code == 500
    assert response.get_json() == {"status": 500}


def test_triangular_route_reports_a_zero_motor_offset(client, calibration, monkeypatch):
    monkeypatch.setattr(calibration.triangularCalibration, "calculate", lambda result: ZERO_TRIANGULAR)
    response = client.post("/triangularCalibration", data=TRIANGULAR)
    assert response.status_code == 200
    assert response.get_json()["data"]["motorYoffset"] == 0.0


def test_holey_route_answers_json_when_the_calculation_fails(client, calibration):
    response = client.post("/holeyCalibration", data=holey_form([""] * 12))
    assert response.status_code == 500
    assert response.get_json() == {"status": 500}


def test_holey_route_reports_a_zero_motor_offset(client, calibration, monkeypatch):
    monkeypatch.setattr(calibration.holeyCalibration, "Calibrate", lambda result: ZERO_HOLEY)
    response = client.post("/holeyCalibration", data=holey_form([1] * 12))
    assert response.status_code == 200
    assert response.get_json()["data"]["motorYoffset"] == 0.0


@pytest.mark.parametrize(
    "path, body, action, value",
    [
        ("/api/calibration/triangular", {"cut12": 1930, "cut34": 1930, "cut5": 300, "bitDiameter": 6.35}, "calibrate", ZERO_TRIANGULAR),
        ("/api/calibration/holey", {"measurements": [1] * 12}, "holeyCalibrate", ZERO_HOLEY),
    ],
)
def test_api_reports_a_zero_motor_offset(client, calibration, monkeypatch, path, body, action, value):
    monkeypatch.setattr(calibration.actions, action, lambda values: value)
    response = client.post(path, json=body)
    assert response.status_code == 200
    assert response.get_json()["motorYoffset"] == 0.0
