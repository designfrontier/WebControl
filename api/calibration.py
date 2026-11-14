"""Triangular, holey and optical calibration data for the new UI; the math stays in Actions/*Calibration.py."""
import math

from flask import Blueprint, current_app, jsonify, request

from app import socketio

blueprint = Blueprint("calibration", __name__, url_prefix="/calibration")

OPTICAL = "Optical Calibration Settings"
HOLEY_MEASUREMENTS = 12
TRIANGULAR_FIELDS = {
    "cut12": "the distance between cut 1 and cut 2",
    "cut34": "the distance between cut 3 and cut 4",
    "cut5": "the distance between the top of the work area and cut 5",
    "bitDiameter": "the bit diameter",
}
OPTICAL_DEFAULTS = {
    "markerX": 0.6,
    "markerY": 0.6,
    "scaleX": 1.0,
    "scaleY": 1.0,
    "opticalCenterX": 0.0,
    "opticalCenterY": 0.0,
    "positionTolerance": 0.125,
    "tlX": -15,
    "tlY": 7,
    "brX": 15,
    "brY": -7,
    "calibrationExtents": "Full Area",
    "autoScanDirection": 0,
    "gaussianBlurValue": 5,
    "cannyLowValue": 50.0,
    "cannyHighValue": 100.0,
}
OPTICAL_INTS = {"tlX", "tlY", "brX", "brY", "autoScanDirection", "gaussianBlurValue"}
OPTICAL_ACTIONS = {"optical_Calibrate", "testOpticalCalibration", "findCenterOpticalCalibration"}


class InvalidInput(Exception):
    pass


def _number(value, default=0.0):
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def _config(data, section, key):
    return _number(data.config.getValue(section, key))


def _error(message, status):
    return jsonify({"error": message}), status


def _measurement(value, label):
    number = None if isinstance(value, bool) or not isinstance(value, (int, float, str)) else _number(value, None)
    if number is None or not math.isfinite(number):
        raise InvalidInput(f"Please enter a number for {label}.")
    return number


def _check_range(value, low, high, label):
    if value < low or value > high:
        raise InvalidInput(f"The {label} of {value} mm seems wrong. Please check the number and enter it again.")


def triangular_inputs(body, data):
    """Mirrors TriangularCalibration.calculate's checks, which only reach the controller message queue."""
    values = {key: _measurement(body.get(key), label) for key, label in TRIANGULAR_FIELDS.items()}
    width = _config(data, "Maslow Settings", "bedWidth")
    height = _config(data, "Maslow Settings", "bedHeight")
    _check_range(values["cut12"], width / 2, width, "measurement between cut 1 and cut 2")
    _check_range(values["cut34"], width / 2, width, "measurement between cut 3 and cut 4")
    _check_range(values["cut5"], 0, height / 2, "measurement between the top edge of the work area and cut 5")
    _check_range(values["bitDiameter"], 0, 25.4, "bit diameter")
    return values


def holey_inputs(body):
    measurements = body.get("measurements")
    if not isinstance(measurements, list) or len(measurements) != HOLEY_MEASUREMENTS:
        raise InvalidInput(f"Please enter all {HOLEY_MEASUREMENTS} measurements.")
    return {f"M{n}": _measurement(value, f"the distance M{n}") for n, value in enumerate(measurements, 1)}


def optical_settings(data):
    def read(key, default):
        value = data.config.getValue(OPTICAL, key)
        if key == "calibrationExtents":
            return str(value) if value else default
        number = _number(value, default)
        return int(number) if key in OPTICAL_INTS else number

    return {key: read(key, default) for key, default in OPTICAL_DEFAULTS.items()}


def optical_inputs(body):
    if not isinstance(body, dict):
        raise InvalidInput("Expected the optical calibration settings.")
    missing = [key for key in OPTICAL_DEFAULTS if key not in body]
    if missing:
        raise InvalidInput(f"Missing {', '.join(missing)}.")
    if not isinstance(body["calibrationExtents"], str):
        raise InvalidInput("Please choose the calibration extents.")
    numbers = {key: _measurement(body[key], key) for key in OPTICAL_DEFAULTS if key != "calibrationExtents"}
    integers = {key: int(numbers[key]) for key in OPTICAL_INTS}
    return {**numbers, **integers, "calibrationExtents": body["calibrationExtents"]}


def optical_matrices(data):
    errors = data.config.getValue(OPTICAL, "xyErrorArray")
    error_x, error_y = data.config.parseErrorArray(errors, True) if errors else ([], [])
    return {
        "curve": {
            "curveX": [_config(data, OPTICAL, f"calX{n}") for n in range(6)],
            "curveY": [_config(data, OPTICAL, f"calY{n}") for n in range(6)],
        },
        "error": {"errorX": error_x, "errorY": error_y},
    }


def _body():
    body = request.get_json(silent=True)
    return body if isinstance(body, dict) else {}


@blueprint.get("/triangular")
def get_triangular():
    data = current_app.data
    return jsonify(
        {
            "motorYoffset": _config(data, "Maslow Settings", "motorOffsetY"),
            "rotationRadius": _config(data, "Advanced Settings", "rotationRadius"),
            "chainSagCorrection": _config(data, "Advanced Settings", "chainSagCorrection"),
        }
    )


@blueprint.post("/triangular")
def calculate_triangular():
    data = current_app.data
    data.logger.resetIdler()
    try:
        values = triangular_inputs(_body(), data)
    except InvalidInput as error:
        return _error(str(error), 400)
    result = data.actions.calibrate(values)
    if result is False:
        return _error("The machine could not be calibrated. Check the work area dimensions and try again.", 422)
    motor_y_offset, rotation_radius, chain_sag_correction, calibration_error = result
    return jsonify(
        {
            "motorYoffset": motor_y_offset,
            "rotationRadius": rotation_radius,
            "chainSagCorrection": chain_sag_correction,
            "calibrationError": calibration_error,
        }
    )


@blueprint.get("/holey")
def get_holey():
    data = current_app.data
    return jsonify(
        {
            "motorYoffset": _config(data, "Maslow Settings", "motorOffsetY"),
            "distanceBetweenMotors": _config(data, "Maslow Settings", "motorSpacingX"),
            "leftChainTolerance": _config(data, "Advanced Settings", "leftChainTolerance"),
            "rightChainTolerance": _config(data, "Advanced Settings", "rightChainTolerance"),
        }
    )


@blueprint.post("/holey")
def calculate_holey():
    data = current_app.data
    data.logger.resetIdler()
    try:
        values = holey_inputs(_body())
    except InvalidInput as error:
        return _error(str(error), 400)
    result = data.actions.holeyCalibrate(values)
    if result is False:
        return _error("The calibration could not be calculated from these measurements.", 422)
    keys = ["motorYoffset", "distanceBetweenMotors", "leftChainTolerance", "rightChainTolerance", "calibrationError"]
    return jsonify({key: float(value) for key, value in zip(keys, result)})


@blueprint.get("/optical")
def get_optical():
    data = current_app.data
    return jsonify({"settings": optical_settings(data), **optical_matrices(data)})


@blueprint.put("/optical/settings")
def save_optical_settings():
    data = current_app.data
    data.logger.resetIdler()
    try:
        values = optical_inputs(request.get_json(silent=True))
    except InvalidInput as error:
        return _error(str(error), 400)
    if not data.opticalCalibration.saveOpticalCalibrationConfiguration(values):
        return _error("The optical calibration settings could not be saved.", 500)
    return jsonify({"settings": optical_settings(data)})


@socketio.on("opticalCalibration", namespace="/MaslowCNC")
def optical_calibration(msg):
    """processAction with the settings object as `arg`, which the shared `action` event types as a scalar."""
    data = current_app.data
    data.logger.resetIdler()
    payload = msg.get("data") if isinstance(msg, dict) else None
    command = payload.get("command") if isinstance(payload, dict) else None
    if command not in OPTICAL_ACTIONS:
        data.console_queue.put(f"Ignoring opticalCalibration request: unknown command {command!r}")
        return
    try:
        settings = optical_inputs(payload.get("arg"))
    except InvalidInput as error:
        data.console_queue.put(f"Ignoring opticalCalibration request: {error}")
        return
    data.actions.processAction({"data": {"command": command, "arg": settings, "arg1": ""}})
