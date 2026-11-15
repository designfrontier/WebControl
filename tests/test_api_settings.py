import copy

import pytest

from api.settings import SECTIONS, describe_setting, is_int, is_number
from helpers import drain


@pytest.fixture
def config(data):
    """Restores every setting, the firmware version and port list the tests change."""
    saved = copy.deepcopy(data.config.settings)
    saved_data = {
        "controllerFirmwareVersion": data.controllerFirmwareVersion,
        "comPorts": data.comPorts,
        "connectionStatus": data.connectionStatus,
        "requestSerialClose": data.requestSerialClose,
    }
    yield data.config
    data.config.settings = saved
    for key, value in saved_data.items():
        setattr(data, key, value)


def by_key(body):
    return {setting["key"]: setting for setting in body["settings"]}


@pytest.mark.parametrize("section_id, title", SECTIONS.items())
def test_each_old_settings_modal_has_a_section(client, config, section_id, title):
    body = client.get(f"/api/settings/{section_id}").get_json()
    assert body["id"] == section_id
    assert body["title"] == title
    assert [setting["key"] for setting in body["settings"]] == [
        setting["key"] for setting in config.getJSONSettingSection(title)
    ]


def test_unknown_section_is_404(client):
    assert client.get("/api/settings/ground-control").status_code == 404
    assert client.put("/api/settings/nope", json={"values": {}}).status_code == 404


def test_settings_carry_their_schema_and_the_default_in_the_description(client, config):
    settings = by_key(client.get("/api/settings/advanced").get_json())
    assert settings["chainOverSprocket"]["type"] == "options"
    assert settings["chainOverSprocket"]["options"] == ["Top", "Bottom"]
    assert settings["truncate"]["type"] == "bool"
    assert settings["encoderSteps"]["numeric"] is True
    assert "default setting:" in settings["encoderSteps"]["desc"]
    assert settings["encoderSteps"]["custom"] is False


def test_numeric_follows_the_type_or_a_numeric_string_value(client, config):
    settings = by_key(client.get("/api/settings/maslow").get_json())
    assert settings["motorSpacingX"]["numeric"] is True
    assert settings["macro1_title"]["numeric"] is False
    assert settings["COMport"]["numeric"] is False
    assert by_key(client.get("/api/settings/webcontrol").get_json())["webPort"]["numeric"] is True


def test_a_setting_not_yet_read_from_the_controller_is_numeric_and_saves_while_blank(client, data, config):
    next(item for item in config.settings["Advanced Settings"] if item["key"] == "maxZRPM")["value"] = ""
    drain(data.gcode_queue)
    assert by_key(client.get("/api/settings/advanced").get_json())["maxZRPM"]["numeric"] is True

    assert client.put("/api/settings/advanced", json={"values": {"truncate": True}}).status_code == 200
    assert client.put("/api/settings/advanced", json={"values": {"maxZRPM": "fast"}}).status_code == 400
    assert not [line for line in drain(data.gcode_queue) if line.startswith("$18")]


def test_maslow_section_lists_the_serial_ports(client, data, config):
    data.comPorts = ["/dev/tty.usbmodem1", "/dev/tty.usbmodem2"]
    body = client.get("/api/settings/maslow").get_json()
    assert body["ports"] == ["/dev/tty.usbmodem1", "/dev/tty.usbmodem2"]
    assert "ports" not in client.get("/api/settings/advanced").get_json()


def test_gpio_pins_offer_unassigned_plus_every_gpio_action(client, data, config):
    settings = client.get("/api/settings/gpio").get_json()["settings"]
    assert settings
    assert all(setting["options"] == [""] + data.gpioActions.getActionList() for setting in settings)


@pytest.mark.parametrize("version, enabled", [(0, False), (99.9, False), (100, True), (101.5, True)])
def test_custom_settings_follow_the_firmware_version(client, data, config, version, enabled):
    data.controllerFirmwareVersion = version
    assert client.get("/api/settings/maslow").get_json()["enableCustom"] is enabled


def test_describe_setting_handles_missing_fields_and_custom_flags(data):
    described = describe_setting("Maslow Settings", {"key": "x", "custom": 1}, data)
    assert described == {
        "key": "x",
        "title": "x",
        "desc": "",
        "type": "string",
        "value": "",
        "custom": True,
        "numeric": False,
    }


@pytest.mark.parametrize(
    "value, expected",
    [
        ("1.5", True),
        (" -2e3 ", True),
        (-2, True),
        ("", False),
        (None, False),
        (True, False),
        ("nan", False),
        ("inf", False),
        ("1_000", False),
        ("1e999", False),
        ("-1e400", False),
        ("\u0663", False),
        (float("nan"), False),
    ],
)
def test_is_number(value, expected):
    assert is_number(value) is expected


@pytest.mark.parametrize("value, expected", [("12", True), (5000, True), ("1.5", False), (False, False)])
def test_is_int(value, expected):
    assert is_int(value) is expected


def test_update_saves_through_config_and_returns_the_fresh_section(client, config):
    body = client.put(
        "/api/settings/maslow", json={"values": {"macro1_title": "Spindle", "zAxis": True, "zAxisSafeHeight": "7.5"}}
    ).get_json()
    assert config.getValue("Maslow Settings", "macro1_title") == "Spindle"
    assert config.getValue("Maslow Settings", "zAxis") == 1
    assert config.getValue("Maslow Settings", "zAxisSafeHeight") == "7.5"
    assert by_key(body)["macro1_title"]["value"] == "Spindle"


def test_update_leaves_settings_it_was_not_sent_alone(client, config):
    before = copy.deepcopy(config.getJSONSettingSection("Advanced Settings"))
    client.put("/api/settings/advanced", json={"values": {"truncate": True}})
    after = config.getJSONSettingSection("Advanced Settings")
    changed = [old["key"] for old, new in zip(before, after) if str(old["value"]) != str(new["value"])]
    assert changed == ["truncate"]


def test_update_turns_bools_off(client, config):
    client.put("/api/settings/maslow", json={"values": {"bufferOn": True}})
    client.put("/api/settings/maslow", json={"values": {"bufferOn": False}})
    assert config.getValue("Maslow Settings", "bufferOn") == 0


def test_typed_numbers_are_stored_with_their_type(client, config):
    client.put("/api/settings/webcontrol", json={"values": {"webPort": "5001", "loggingTimeout": "2.5"}})
    assert config.getValue("WebControl Settings", "webPort") == 5001
    assert config.getValue("WebControl Settings", "loggingTimeout") == 2.5


def test_changing_a_firmware_setting_queues_it_for_the_controller(client, data, config):
    drain(data.gcode_queue)
    client.put("/api/settings/maslow", json={"values": {"motorSpacingX": "3000"}})
    assert any(line.startswith("$2=") for line in drain(data.gcode_queue))


def test_changing_the_port_while_connected_closes_the_serial_link(client, data, config):
    data.connectionStatus = 1
    data.requestSerialClose = False
    client.put("/api/settings/maslow", json={"values": {"COMport": "/dev/tty.other"}})
    assert data.requestSerialClose is True
    assert config.getValue("Maslow Settings", "COMport") == "/dev/tty.other"


@pytest.mark.parametrize("key", ["motorSpacingX", "sledWeight"])
@pytest.mark.parametrize("value", ["nan", "1e999", "-1e400"])
def test_non_finite_numbers_never_reach_the_controller(client, data, config, key, value):
    drain(data.gcode_queue)
    saved = config.getValue("Maslow Settings", key)
    response = client.put("/api/settings/maslow", json={"values": {key: value}})
    assert response.get_json() == {"errors": {key: "Must be a number"}}
    assert drain(data.gcode_queue) == []
    assert config.getValue("Maslow Settings", key) == saved


def test_camera_changes_reach_the_running_camera(client, data, config):
    saved_fps = data.camera.fps
    client.put("/api/settings/camera", json={"values": {"fps": "7"}})
    assert data.camera.fps == 7.0
    data.camera.fps = saved_fps


def test_gpio_assignment_is_saved(client, config):
    client.put("/api/settings/gpio", json={"values": {"gpio4": "Stop"}})
    assert config.getValue("GPIO Settings", "gpio4") == "Stop"
    client.put("/api/settings/gpio", json={"values": {"gpio4": ""}})
    assert config.getValue("GPIO Settings", "gpio4") == ""


@pytest.mark.parametrize(
    "section, values, errors",
    [
        ("maslow", {"nope": "1"}, {"nope": "Unknown setting"}),
        ("maslow", {"motorSpacingX": "wide", "macro1_title": "Ok"}, {"motorSpacingX": "Must be a number"}),
        ("maslow", {"zAxis": "yes"}, {"zAxis": "Must be true or false"}),
        ("maslow", {"macro1": ["G0"]}, {"macro1": "Must be text or a number"}),
        ("advanced", {"chainOverSprocket": "Sideways"}, {"chainOverSprocket": "Must be one of the listed options"}),
        ("webcontrol", {"webPort": "50.5"}, {"webPort": "Must be a whole number"}),
        ("gpio", {"gpio4": "Explode"}, {"gpio4": "Must be one of the listed options"}),
    ],
)
def test_invalid_updates_are_rejected_without_saving_anything(client, config, section, values, errors):
    before = copy.deepcopy(config.settings)
    response = client.put(f"/api/settings/{section}", json={"values": values})
    assert response.status_code == 400
    assert response.get_json() == {"errors": errors}
    assert config.settings == before


@pytest.mark.parametrize("body", [None, [], {"values": []}, {"other": {}}])
def test_malformed_bodies_are_rejected(client, config, body):
    response = client.put("/api/settings/maslow", json=body)
    assert response.status_code == 400
    assert "error" in response.get_json()


def test_custom_settings_are_locked_without_custom_firmware(client, data, config):
    entry = next(s for s in config.settings["Maslow Settings"] if s["key"] == "macro2")
    entry["custom"] = 1
    data.controllerFirmwareVersion = 0
    response = client.put("/api/settings/maslow", json={"values": {"macro2": "G0 X0"}})
    assert response.status_code == 400
    assert response.get_json()["errors"]["macro2"].startswith("Needs custom firmware")

    data.controllerFirmwareVersion = 100
    assert client.put("/api/settings/maslow", json={"values": {"macro2": "G0 X0"}}).status_code == 200
    assert config.getValue("Maslow Settings", "macro2") == "G0 X0"
