import copy
import json
import math

import pytest

from Actions.HoleySimulationKinematics import Kinematics
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
