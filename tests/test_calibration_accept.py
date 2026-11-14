import re

from calibration_helpers import calibration, holey_form, ideal_holey_measurements  # noqa: F401
from helpers import drain

TRIANGULAR = {"cut12": 1930, "cut34": 1930, "cut5": 300, "bitDiameter": 6.35}


def settings(data):
    return {
        "motorOffsetY": data.config.getValue("Maslow Settings", "motorOffsetY"),
        "motorSpacingX": data.config.getValue("Maslow Settings", "motorSpacingX"),
        "leftChainTolerance": data.config.getValue("Advanced Settings", "leftChainTolerance"),
        "rightChainTolerance": data.config.getValue("Advanced Settings", "rightChainTolerance"),
        "rotationRadius": data.config.getValue("Advanced Settings", "rotationRadius"),
        "chainSagCorrection": data.config.getValue("Advanced Settings", "chainSagCorrection"),
    }


def holey_opt(holey):
    return (holey.Opt_D, holey.Opt_motorOffsetY, holey.Opt_leftChainTolerance, holey.Opt_rightChainTolerance)


def test_holey_accept_refuses_without_a_calculation(calibration):
    before = settings(calibration)
    assert calibration.holeyCalibration.acceptCalibrationResults() is False
    assert settings(calibration) == before
    assert drain(calibration.gcode_queue) == []


def test_holey_blank_measurement_leaves_the_previous_values_alone(calibration):
    measurements = ideal_holey_measurements(calibration)
    measurements[4] = ""
    before = holey_opt(calibration.holeyCalibration)
    assert calibration.holeyCalibration.Calibrate(holey_form(measurements)) is None
    assert holey_opt(calibration.holeyCalibration) == before
    assert calibration.holeyCalibration.hasResult is False
    assert calibration.holeyCalibration.acceptCalibrationResults() is False


def test_holey_accept_writes_a_successful_calculation_once(calibration):
    holey = calibration.holeyCalibration
    assert holey.Calibrate(holey_form(ideal_holey_measurements(calibration))) is not None
    assert holey.hasResult is True
    assert holey.acceptCalibrationResults() is True
    assert float(calibration.config.getValue("Maslow Settings", "motorSpacingX")) == holey.Opt_D
    assert holey.hasResult is False
    assert holey.acceptCalibrationResults() is False


def test_holey_failed_recalculation_withdraws_the_earlier_result(calibration):
    holey = calibration.holeyCalibration
    measurements = ideal_holey_measurements(calibration)
    holey.Calibrate(holey_form(measurements))
    holey.Calibrate(holey_form(measurements[:-1] + ["x"]))
    assert holey.acceptCalibrationResults() is False


def test_triangular_accept_refuses_without_a_calculation(calibration):
    before = settings(calibration)
    assert calibration.triangularCalibration.acceptTriangularCalibrationResults() is False
    assert settings(calibration) == before
    assert drain(calibration.gcode_queue) == []


def test_triangular_failed_recalculation_withdraws_the_earlier_result(calibration):
    triangular = calibration.triangularCalibration
    assert triangular.calculate(TRIANGULAR)
    assert triangular.hasResult is True
    assert not triangular.calculate({**TRIANGULAR, "cut12": "nope"})
    assert triangular.acceptTriangularCalibrationResults() is False


def test_triangular_accept_writes_a_successful_calculation(calibration):
    triangular = calibration.triangularCalibration
    motor_y_offset, rotation_radius, chain_sag, _ = triangular.calculate(TRIANGULAR)
    assert triangular.acceptTriangularCalibrationResults() is True
    assert float(calibration.config.getValue("Maslow Settings", "motorOffsetY")) == motor_y_offset
    assert float(calibration.config.getValue("Advanced Settings", "rotationRadius")) == rotation_radius
    assert "G0 X0 Y0 " in drain(calibration.gcode_queue)
    assert triangular.acceptTriangularCalibrationResults() is False


def test_api_holey_accept_refuses_without_a_calculation(client, calibration):
    response = client.post("/api/calibration/holey/accept")
    assert response.status_code == 409
    assert "Calculate" in response.get_json()["error"]


def test_api_holey_accept_saves_the_calculated_values(client, calibration):
    measurements = ideal_holey_measurements(calibration)
    calculated = client.post("/api/calibration/holey", json={"measurements": measurements}).get_json()
    response = client.post("/api/calibration/holey/accept")
    assert response.status_code == 200
    assert response.get_json() == {
        "motorYoffset": calculated["motorYoffset"],
        "distanceBetweenMotors": calculated["distanceBetweenMotors"],
        "leftChainTolerance": calculated["leftChainTolerance"],
        "rightChainTolerance": calculated["rightChainTolerance"],
    }


def test_api_triangular_accept_refuses_without_a_calculation(client, calibration):
    response = client.post("/api/calibration/triangular/accept")
    assert response.status_code == 409


def test_api_triangular_accept_saves_the_calculated_values(client, calibration):
    calculated = client.post("/api/calibration/triangular", json=TRIANGULAR).get_json()
    response = client.post("/api/calibration/triangular/accept")
    assert response.status_code == 200
    assert response.get_json() == {
        "motorYoffset": calculated["motorYoffset"],
        "rotationRadius": calculated["rotationRadius"],
        "chainSagCorrection": calculated["chainSagCorrection"],
    }


def accept_button(page):
    return re.search(r'<button id="acceptCalibration"[^>]*>', page).group(0)


def test_classic_accept_buttons_start_disabled(app, monkeypatch):
    monkeypatch.setattr("WebPageProcessor.webPageProcessor.socketio.emit", lambda *args, **kwargs: None)
    with app.test_request_context():
        pages = [app.webPageProcessor.createWebPage(page, False, None)[0] for page in ("triangularCalibration", "holeyCalibration")]
    assert all(" disabled" in accept_button(page) for page in pages)
    assert all('$("#acceptCalibration").prop("disabled", true)' in page for page in pages)
