import pytest

from helpers import drain

CALIBRATED_SETTINGS = [
    ("Maslow Settings", "motorOffsetY"),
    ("Maslow Settings", "motorSpacingX"),
    ("Advanced Settings", "leftChainTolerance"),
    ("Advanced Settings", "rightChainTolerance"),
    ("Advanced Settings", "rotationRadius"),
    ("Advanced Settings", "chainSagCorrection"),
]


def holey_form(measurements):
    return {f"M{n}": value for n, value in enumerate(measurements, 1)}


def ideal_holey_measurements(data):
    return [float(value) for value in data.holeyCalibration.InitializeIdealXyCoordinates()]


@pytest.fixture
def calibration(data):
    """Restores every setting an Accept writes and forgets any calculated result."""
    saved = {key: data.config.getValue(*key) for key in CALIBRATED_SETTINGS}
    data.holeyCalibration.hasResult = False
    data.triangularCalibration.hasResult = False
    yield data
    for (section, key), value in saved.items():
        data.config.setValue(section, key, value)
    data.holeyCalibration.hasResult = False
    data.triangularCalibration.hasResult = False
    drain(data.gcode_queue)
    drain(data.message_queue)
