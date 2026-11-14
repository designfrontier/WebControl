import copy
import json
import math

import pytest

from Actions.HoleySimulationKinematics import Kinematics
from Background.UIProcessor import UIProcessor
from calibration_fixture_cases import simulator
from calibration_helpers import holey_form, ideal_holey_measurements
from helpers import drain


@pytest.fixture
def settings_saved(data):
    """Restores the settings, the simulator and the stored webcontrol.json that a settings save changes."""
    saved = copy.deepcopy(data.config.settings)
    simulator = dict(vars(data.holeyKinematics))
    yield data
    data.config.settings = saved
    data.config.setValue("Advanced Settings", "gearTeeth", data.config.getValue("Advanced Settings", "gearTeeth"))
    for key in list(vars(data.holeyKinematics)):
        if key not in simulator:
            delattr(data.holeyKinematics, key)
    vars(data.holeyKinematics).update(simulator)
    drain(data.gcode_queue)


def stored(data, section, key):
    with open(data.config.home + "/.WebControl/webcontrol.json") as infile:
        return next(option["value"] for option in json.load(infile)[section] if option["key"] == key)


@pytest.mark.parametrize(
    "key, value, attribute, expected",
    [
        (2, "2990.5", "D", 2990.5),
        (13, "63.5", "R", 63.5 / math.pi / 2),
        (13, 76.2, "R", 76.2 / math.pi / 2),
        (1, "1219.2", "machineHeight", 1219.2),
        (0, "2438.4", "machineWidth", 2438.4),
        (3, "470", "motorOffsetY", 470.0),
        (40, "0.25", "leftChainTolerance", 0.25),
        (41, "-0.25", "rightChainTolerance", -0.25),
        (45, "5.1685e-6", "chainElasticity", 5.1685e-6),
        (46, "97.9", "sledWeight", 97.9),
        (8, "139.1", "rotationDiskRadius", 139.1),
        (37, "33", "chainSagCorrection", 33.0),
        (38, "2", "chainOverSprocket", 2),
        (38, "2.0", "chainOverSprocket", 2),
    ],
)
def test_simulator_accepts_the_stored_string_values(key, value, attribute, expected):
    kinematics = Kinematics()
    kinematics.updateSetting(key, value)
    assert getattr(kinematics, attribute) == pytest.approx(expected, rel=1e-15)


def test_simulator_kinematics_type_accepts_strings():
    kinematics = Kinematics()
    kinematics.updateSetting(7, "2")
    assert kinematics.isQuadKinematics is False
    kinematics.updateSetting(7, "1")
    assert kinematics.isQuadKinematics is True


def test_simulator_ignores_a_value_it_cannot_read(data):
    kinematics = Kinematics()
    kinematics.data = data
    kinematics.updateSetting(13, "not a number")
    assert kinematics.R == Kinematics.R


@pytest.mark.parametrize("key, value", [("gearTeeth", "12"), ("chainPitch", "6.5")])
def test_changing_the_sprocket_saves_the_whole_form(client, settings_saved, key, value):
    data = settings_saved
    form = {option["key"]: option["value"] for option in data.config.getJSONSettingSection("Advanced Settings")}
    form = {**form, key: value, "rotationRadius": "140.5"}
    response = client.post("/advancedSettings", data=form)
    assert response.status_code == 200
    assert data.config.getValue("Advanced Settings", "rotationRadius") == "140.5"
    assert stored(data, "Advanced Settings", key) == value
    distance_per_rotation = float(data.config.getValue("Computed Settings", "distPerRot"))
    assert data.holeyKinematics.R == pytest.approx(distance_per_rotation / math.pi / 2)


def test_changing_the_sprocket_through_the_settings_api(client, settings_saved):
    data = settings_saved
    response = client.put("/api/settings/advanced", json={"values": {"gearTeeth": 11}})
    assert response.status_code == 200
    assert stored(data, "Advanced Settings", "gearTeeth") == "11"
    assert data.holeyKinematics.R == pytest.approx(11 * 6.35 / math.pi / 2)


def triangular():
    kinematics = Kinematics()
    kinematics.isQuadKinematics = False
    kinematics.recomputeGeometry()
    return kinematics


def test_forward_inverts_inverse_inside_the_work_area():
    kinematics = triangular()
    x, y = kinematics.forward(*kinematics.inverse(-600.0, 300.0))
    assert (x, y) == (pytest.approx(-600.0, abs=1e-6), pytest.approx(300.0, abs=1e-6))


@pytest.mark.parametrize("chains", [(100.0, 100.0), (4000.0, 100.0)])
def test_forward_returns_none_when_it_cannot_converge(chains):
    assert triangular().forward(*chains) is None


@pytest.mark.parametrize("target", [(-1219.2, 609.6), (1219.2, 0.0), (0.0, -609.6)])
def test_forward_finds_points_on_the_edge_of_the_work_area(target):
    kinematics = triangular()
    x, y = kinematics.forward(*kinematics.inverse(*target))
    assert (x, y) == (pytest.approx(target[0], abs=1e-6), pytest.approx(target[1], abs=1e-6))


def test_clamp_returns_the_target_on_the_edge_of_the_work_area():
    kinematics = triangular()
    assert kinematics._verifyValidTarget(-5000.0, 10.0) == (-1219.2, 10.0)
    assert kinematics._verifyValidTarget(20.0, -5000.0) == (20.0, -609.6)
    assert kinematics.inverse(5000.0, 10.0) == kinematics.inverse(1219.2, 10.0)


def test_clamp_moves_both_axes_like_the_firmware():
    kinematics = triangular()
    assert kinematics._verifyValidTarget(-5000.0, 5000.0) == (-1219.2, 609.6)
    assert kinematics._verifyValidTarget(5000.0, -5000.0) == (1219.2, -609.6)
    assert kinematics.inverse(-5000.0, 5000.0) == kinematics.inverse(-1219.2, 609.6)


def test_singular_quadrilateral_jacobian_does_not_raise():
    kinematics = Kinematics()
    kinematics.Jac = [0.0] * 10
    assert kinematics._MatSolv() is False


def test_quadrilateral_forward_gives_up_on_a_singular_jacobian(monkeypatch):
    kinematics = Kinematics()
    monkeypatch.setattr(kinematics, "_MatSolv", lambda: False)
    kinematics.Phi = 1.0
    assert kinematics.forward(1740.0, 1740.0) is None


def test_quadrilateral_forward_still_converges():
    kinematics = Kinematics()
    x, y = kinematics.forward(*kinematics.inverse(300.0, -200.0))
    assert (x, y) == (pytest.approx(300.0, abs=0.01), pytest.approx(-200.0, abs=0.01))


def test_quadrilateral_forward_places_the_sled_on_the_centre_line():
    kinematics = simulator(True)
    ys = [float(y) for y in range(-600, 625, 25)]
    found = [kinematics.forward(*kinematics.inverse(0.0, y), 0.0, y) for y in ys]
    assert all(position is not None for position in found)
    assert all(math.hypot(x, y - target) < 0.1 for (x, y), target in zip(found, ys))


def test_error_report_hides_an_unknown_computed_position(app, data, monkeypatch):
    processor = UIProcessor()
    processor.app = app
    sent = []
    settings = {"positionErrorLimit": "2.0", "computedPosition": "1"}
    real_get_value = data.config.getValue
    monkeypatch.setattr(data.config, "getValue", lambda section, key: settings.get(key) or real_get_value(section, key))
    monkeypatch.setattr(data, "controllerFirmwareVersion", 100)
    touched = ["leftError", "rightError", "leftError_prev", "rightError_prev", "computedX", "computedY"]
    for key in [*touched, "leftChain", "rightChain", "bufferSize"]:
        monkeypatch.setattr(data, key, getattr(data, key))
    monkeypatch.setattr(data.holeyKinematics, "forward", lambda *args: None)
    monkeypatch.setattr(processor, "sendErrorValueMessage", sent.append)
    processor.setErrorOnScreen("[PE:0.5,0.25,127,100.0,100.0]")
    assert sent[0]["computedX"] == -999999
    assert sent[0]["computedY"] == -999999


def test_holey_fit_refuses_when_the_kinematics_cannot_place_a_hole(data, monkeypatch):
    measurements = ideal_holey_measurements(data)
    monkeypatch.setattr(data.holeyCalibration.kin, "forward", lambda *args: None)
    monkeypatch.setattr(data.holeyCalibration, "hasResult", False)
    assert data.holeyCalibration.Calibrate(holey_form(measurements)) is None
    assert data.holeyCalibration.hasResult is False
    assert "could not be calculated" in data.message_queue.get_nowait()
