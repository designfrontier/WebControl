"""Holey calibration fits with the machine model the firmware runs: the user's sprocket, chain and sled settings."""
import copy
import math

import pytest

import Actions.HoleySimulationKinematics as holeyKinematics
from Actions.HoleySimulationKinematics import FIRMWARE_PI, Kinematics
from calibration_helpers import holey_form
from helpers import drain

USER_SETTINGS = {
    "Maslow Settings": {
        "motorSpacingX": "3011.16237", "motorOffsetY": "433.68186", "bedWidth": "2438.4", "bedHeight": "1219.2",
        "sledWeight": "97.9", "sledWidth": "310", "sledHeight": "139", "sledCG": "79",
    },
    "Advanced Settings": {
        "leftChainTolerance": "-0.06495", "rightChainTolerance": "-0.02111", "chainElasticity": "5.1685e-06",
        "rotationRadius": "138.4", "chainOverSprocket": "Bottom", "gearTeeth": "10", "chainPitch": "6.35",
        "kinematicsType": "Triangular",
    },
}
CONFIGURED = {"D": 3011.16237, "motorOffsetY": 433.68186, "leftChainTolerance": -0.06495, "rightChainTolerance": -0.02111}


@pytest.fixture
def holey(data):
    saved = copy.deepcopy(data.config.settings)
    simulator = dict(vars(data.holeyKinematics))
    model = dict(vars(data.holeyCalibration.kin))
    for section, values in USER_SETTINGS.items():
        for key, value in values.items():
            data.config.setValue(section, key, value)
    drain(data.gcode_queue)
    yield data.holeyCalibration
    data.config.settings = saved
    vars(data.holeyKinematics).clear()
    vars(data.holeyKinematics).update(simulator)
    vars(data.holeyCalibration.kin).clear()
    vars(data.holeyCalibration.kin).update(model)
    data.holeyCalibration.hasResult = False
    for queue in (data.gcode_queue, data.message_queue):
        drain(queue)


def set_setting(data, section, key, value):
    data.config.setValue(section, key, value)
    drain(data.gcode_queue)


def test_firmware_pi_is_the_literal_the_firmware_uses():
    assert FIRMWARE_PI == 3.14159


def test_the_fit_uses_the_firmware_sprocket_radius(holey, monkeypatch):
    seen = []
    fit_positions = holey.HolePositionsFromChainLengths
    monkeypatch.setattr(holey, "HolePositionsFromChainLengths", lambda: seen.append(holey.kin.R) or fit_positions())

    holey.Calibrate(holey_form(holey.InitializeIdealXyCoordinates()))

    assert seen and set(seen) == {63.5 / (2 * 3.14159)}


def test_the_sprocket_radius_follows_the_gear_and_chain_settings(holey):
    set_setting(holey.data, "Advanced Settings", "gearTeeth", "12")

    holey.InitializeIdealXyCoordinates()

    assert holey.kin.R == 12 * 6.35 / (2 * 3.14159)


def test_the_fit_uses_the_chain_elasticity_setting(holey):
    set_setting(holey.data, "Advanced Settings", "chainElasticity", "6.2e-06")

    holey.InitializeIdealXyCoordinates()

    assert holey.kin.chainElasticity == 6.2e-06


def test_the_fit_uses_the_sled_geometry_settings(holey):
    [set_setting(holey.data, "Maslow Settings", key, value) for key, value in
     [("sledWidth", "300"), ("sledHeight", "120"), ("sledCG", "70")]]

    holey.InitializeIdealXyCoordinates()

    kin = holey.kin
    assert (kin.l, kin.s, kin.h3) == (300.0, 120.0, 70.0)
    assert kin.h == pytest.approx(math.sqrt(150.0 ** 2 + 120.0 ** 2), rel=1e-15)
    assert kin.Theta == pytest.approx(math.atan(2 * 120.0 / 300.0), rel=1e-15)


def test_the_fit_uses_a_chain_density_setting_when_there_is_one(holey, monkeypatch):
    real_get_value = holey.data.config.getValue
    monkeypatch.setattr(holey.data.config, "getValue", lambda section, key: (
        "0.0015" if key == "chainDensity" else real_get_value(section, key)))

    holey.InitializeIdealXyCoordinates()

    assert holey.kin.chainDensity == 0.0015


def test_settings_that_are_missing_keep_the_model_defaults(holey, monkeypatch):
    real_get_value = holey.data.config.getValue
    missing = {"chainElasticity", "sledWidth", "sledHeight", "sledCG", "chainDensity"}
    monkeypatch.setattr(holey.data.config, "getValue", lambda section, key: (
        None if key in missing else real_get_value(section, key)))

    holey.InitializeIdealXyCoordinates()

    kin = holey.kin
    assert (kin.chainElasticity, kin.l, kin.s, kin.h3, kin.chainDensity) == (
        Kinematics.chainElasticity, Kinematics.l, Kinematics.s, Kinematics.h3, Kinematics.chainDensity)


def firmware_machine(params):
    kin = Kinematics()
    kin.isQuadKinematics = False
    kin.R = 63.5 / (2 * 3.14159)
    kin.rotationDiskRadius = 138.4
    kin.chainOverSprocket = 2
    for key, value in params.items():
        setattr(kin, key, value)
    kin.recomputeGeometry()
    return kin


def firmware_measurements(holey, true_params):
    """The holes a machine with `true_params` cuts when the firmware drives it with the configured settings."""
    chains = [firmware_machine(CONFIGURED).inverse(x, y) for x, y in holey.IdealCoordinates]
    true_machine = firmware_machine(true_params)
    holes = [true_machine.forward(left, right) for left, right in chains]
    lengths = [holey.GeometricLength(*holes[a - 1], *holes[b - 1]) for a, b in holey.MeasurementMap]
    return [*lengths, holey.GeometricLength(*holes[1], holes[1][0], 1219.2 / 2)]


def test_the_fit_recovers_a_machine_the_firmware_drives(holey):
    true_params = {"D": 3022.76237, "motorOffsetY": 440.68186, "leftChainTolerance": 0.23505, "rightChainTolerance": -0.22111}
    holey.InitializeIdealXyCoordinates()

    offset_y, distance, left, right, _ = holey.Calibrate(holey_form(firmware_measurements(holey, true_params)))

    assert distance == pytest.approx(true_params["D"], abs=2e-4)
    assert offset_y == pytest.approx(true_params["motorOffsetY"], abs=2e-4)
    assert left == pytest.approx(true_params["leftChainTolerance"], abs=2e-5)
    assert right == pytest.approx(true_params["rightChainTolerance"], abs=2e-5)


def test_the_simulator_sets_the_sprocket_radius_like_the_firmware(data):
    kinematics = Kinematics()
    kinematics.updateSetting(13, "63.5")

    assert kinematics.R == 63.5 / (2 * 3.14159)


def test_the_chain_wrap_uses_the_firmware_pi(monkeypatch):
    kinematics = firmware_machine(CONFIGURED)
    wrapped = kinematics.inverse(-600.0, 300.0)
    monkeypatch.setattr(holeyKinematics, "FIRMWARE_PI", 3.0)

    assert kinematics.inverse(-600.0, 300.0) != wrapped
