from types import SimpleNamespace

import pytest

from api.machine import machine_setup, parse_quick_configure, quick_configure

SETTINGS = {
    ("Advanced Settings", "chainExtendLength"): "1650",
    ("Advanced Settings", "rotationRadius"): "138.4",
    ("Advanced Settings", "kinematicsType"): "Triangular",
    ("Advanced Settings", "chainOverSprocket"): "Bottom",
    ("Maslow Settings", "motorSpacingX"): "2978.4",
    ("Maslow Settings", "motorOffsetY"): 463,
}

QUICK_KEYS = [
    ("Advanced Settings", "kinematicsType"),
    ("Advanced Settings", "rotationRadius"),
    ("Advanced Settings", "chainOverSprocket"),
    ("Maslow Settings", "motorSpacingX"),
    ("Maslow Settings", "motorOffsetY"),
]

VALID = {
    "kinematicsType": "Linkage",
    "rotationRadius": 273,
    "chainOverSprocket": "Top",
    "motorSpacingX": 2938.05,
    "motorOffsetY": 470,
}


def fake_config(overrides=None):
    settings = {**SETTINGS, **(overrides or {})}
    return SimpleNamespace(getValue=lambda section, key: settings.get((section, key)))


def fake_data(version=0, overrides=None, rewrite_notice=False):
    return SimpleNamespace(
        config=fake_config(overrides), controllerFirmwareVersion=version, firmwareRewriteNoticeSeen=rewrite_notice
    )


@pytest.mark.parametrize(
    "version, four_motor, z_limits",
    [(0, False, False), (1.26, False, False), (1.27, False, False), (51.27, False, False), (51.29, False, False),
     (51.293, False, True), (100, True, False), ("51.2", False, False)],
)
def test_setup_derives_capabilities_from_the_firmware_version(version, four_motor, z_limits):
    setup = machine_setup(fake_data(version))
    assert setup["fourMotor"] is four_motor
    assert setup["firmwareSupportsZaxisLimit"] is z_limits
    assert setup["controllerFirmwareVersion"] == float(version)


def test_setup_reports_chain_length_and_touch_plate():
    assert machine_setup(fake_data(1.3)) == {
        "chainExtendLength": 1650.0,
        "fourMotor": False,
        "firmwareSupportsZaxisLimit": False,
        "touchPlate": False,
        "controllerFirmwareVersion": 1.3,
        "firmwareBuild": "1.3",
    }
    touch = machine_setup(fake_data(0, {("Advanced Settings", "touchPlate"): 1}))
    assert touch["touchPlate"] is True


def test_z_limits_need_51_292_which_51_29_proves_with_its_eeprom_rewrite_notice():
    assert machine_setup(fake_data(51.29, rewrite_notice=True))["firmwareSupportsZaxisLimit"] is True
    assert machine_setup(fake_data(51.29, rewrite_notice=True))["firmwareBuild"] == "51.292"


def test_setup_survives_unparseable_values():
    setup = machine_setup(fake_data("nan", {("Advanced Settings", "chainExtendLength"): "abc"}))
    assert setup["chainExtendLength"] == 0.0
    assert setup["controllerFirmwareVersion"] == 0.0


def test_quick_configure_reads_a_ring_kit():
    assert quick_configure(fake_config()) == {
        "kinematicsType": "Ring",
        "rotationRadius": 138.4,
        "chainOverSprocket": "Bottom",
        "motorSpacingX": 2978.4,
        "motorOffsetY": 463.0,
    }


def test_quick_configure_reads_other_triangular_kits_as_custom():
    config = fake_config({("Advanced Settings", "rotationRadius"): "273"})
    assert quick_configure(config)["kinematicsType"] == "Custom"


def test_quick_configure_keeps_quadrilateral_and_defaults_chain_side():
    config = fake_config({("Advanced Settings", "kinematicsType"): "Quadrilateral", ("Advanced Settings", "chainOverSprocket"): None})
    state = quick_configure(config)
    assert state["kinematicsType"] == "Quadrilateral"
    assert state["chainOverSprocket"] == "Top"


def test_parse_turns_numbers_into_the_text_the_old_form_posted():
    form, error = parse_quick_configure(VALID)
    assert error is None
    assert form == {
        "kinematicsType": "Linkage",
        "chainOverSprocket": "Top",
        "rotationRadius": "273",
        "motorSpacingX": "2938.05",
        "motorOffsetY": "470",
    }


@pytest.mark.parametrize(
    "body, message",
    [
        (None, "JSON object"),
        ([], "JSON object"),
        ({**VALID, "kinematicsType": "Triangle"}, "kinematicsType"),
        ({**VALID, "chainOverSprocket": "Left"}, "chainOverSprocket"),
        ({**VALID, "rotationRadius": "273"}, "rotationRadius"),
        ({**VALID, "motorSpacingX": True}, "motorSpacingX"),
        ({**VALID, "motorOffsetY": -1}, "motorOffsetY"),
        ({**VALID, "motorOffsetY": float("inf")}, "motorOffsetY"),
    ],
)
def test_parse_rejects_bad_bodies(body, message):
    form, error = parse_quick_configure(body)
    assert form is None
    assert message in error


def test_setup_endpoint_reads_the_live_config(client, data):
    body = client.get("/api/machine/setup").get_json()
    assert body["chainExtendLength"] == float(data.config.getValue("Advanced Settings", "chainExtendLength"))
    assert body["fourMotor"] is False
    assert body["touchPlate"] is False


@pytest.fixture
def saved_quick_settings(data):
    saved = {key: data.config.getValue(*key) for key in QUICK_KEYS}
    yield
    for (section, key), value in saved.items():
        data.config.setValue(section, key, value)


def test_quick_configure_endpoints_round_trip(client, data, saved_quick_settings):
    response = client.post("/api/machine/quick-configure", json=VALID)
    assert response.status_code == 200
    assert response.get_json() == {
        "kinematicsType": "Custom",
        "rotationRadius": 273.0,
        "chainOverSprocket": "Top",
        "motorSpacingX": 2938.05,
        "motorOffsetY": 470.0,
    }
    assert data.config.getValue("Advanced Settings", "kinematicsType") == "Triangular"
    assert client.get("/api/machine/quick-configure").get_json() == response.get_json()


def test_quick_configure_endpoint_rejects_invalid_bodies(client, data, saved_quick_settings):
    before = client.get("/api/machine/quick-configure").get_json()
    response = client.post("/api/machine/quick-configure", json={**VALID, "chainOverSprocket": "Sideways"})
    assert response.status_code == 400
    assert "chainOverSprocket" in response.get_json()["error"]
    assert client.get("/api/machine/quick-configure").get_json() == before
