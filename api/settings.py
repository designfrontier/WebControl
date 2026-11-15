"""Settings screens for the new frontend: one JSON section per old settings modal, saved through Config.updateSettings."""
import math
import re

from flask import Blueprint, abort, current_app, jsonify, request

from Actions.firmware import isSerialPortName

blueprint = Blueprint("settings", __name__)

SECTIONS = {
    "maslow": "Maslow Settings",
    "advanced": "Advanced Settings",
    "webcontrol": "WebControl Settings",
    "camera": "Camera Settings",
    "gpio": "GPIO Settings",
}

# The old templates disable settings flagged custom unless the controller runs firmware 100 or later.
CUSTOM_FIRMWARE_VERSION = 100


# Matches the frontend's check; float() alone would also accept "nan", "inf", "1_000" and non-ASCII digits.
NUMBER = re.compile(r"^\s*[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?\s*$", re.ASCII)


def is_number(value):
    if isinstance(value, bool):
        return False
    if isinstance(value, (int, float)):
        return math.isfinite(value)
    return isinstance(value, str) and NUMBER.match(value) is not None and math.isfinite(float(value))


def is_int(value):
    if isinstance(value, bool):
        return False
    try:
        int(str(value))
        return True
    except ValueError:
        return False


def custom_enabled(data):
    return data.controllerFirmwareVersion >= CUSTOM_FIRMWARE_VERSION


def setting_options(section, setting, data):
    if section == "GPIO Settings":
        return [""] + list(data.gpioActions.getActionList())
    return setting.get("options")


def describe_setting(section, setting, data):
    """One setting as the form needs it; `numeric` mirrors the old template's isnumber check."""
    kind = setting.get("type", "string")
    # A setting not yet read from the controller is blank, but numeric all the same.
    value_or_default = setting.get("value") if setting.get("value") != "" else setting.get("default")
    described = {
        "key": setting["key"],
        "title": setting.get("title", setting["key"]),
        "desc": setting.get("desc", ""),
        "type": kind,
        "value": setting.get("value", ""),
        "custom": setting.get("custom") == 1,
        "numeric": kind in ("int", "float") or (kind == "string" and is_number(value_or_default)),
    }
    options = setting_options(section, setting, data)
    return {**described, "options": options} if options is not None else described


def section_payload(section_id, data):
    section = SECTIONS[section_id]
    settings = data.config.getJSONSettingSection(section)
    payload = {
        "id": section_id,
        "title": section,
        "enableCustom": custom_enabled(data),
        "settings": [describe_setting(section, setting, data) for setting in settings],
    }
    return {**payload, "ports": list(data.comPorts)} if section_id == "maslow" else payload


def setting_error(setting, value, enable_custom, known_ports=()):
    if setting["key"] == "COMport" and value != "" and not isSerialPortName(value, known_ports):
        return "Must be a serial port such as /dev/ttyACM0 or COM3"
    if setting["custom"] and not enable_custom:
        return "Needs custom firmware (version 100 or later)"
    if setting["type"] == "bool":
        return None if isinstance(value, bool) or value in (0, 1) else "Must be true or false"
    if not isinstance(value, (str, int, float)) or isinstance(value, bool):
        return "Must be text or a number"
    if "options" in setting and value not in setting["options"]:
        return "Must be one of the listed options"
    if setting["type"] == "int" and not is_int(value):
        return "Must be a whole number"
    if setting["numeric"] and not is_number(value):
        return "Must be a number"
    return None


def form_value(setting, value):
    """What the old HTML form would have posted, so Config.updateSettings sees the same input."""
    return bool(value) if setting["type"] == "bool" else str(value)


def validate(described, values, enable_custom, known_ports=()):
    by_key = {setting["key"]: setting for setting in described}
    unknown = {key: "Unknown setting" for key in values if key not in by_key}
    invalid = {
        key: error
        for key, value in values.items()
        if key in by_key
        for error in [setting_error(by_key[key], value, enable_custom, known_ports)]
        if error
    }
    return {**unknown, **invalid}


def section_id_or_404(section_id):
    if section_id not in SECTIONS:
        abort(404)
    return section_id


@blueprint.get("/settings/<section_id>")
def get_section(section_id):
    data = current_app.data
    data.logger.resetIdler()
    return jsonify(section_payload(section_id_or_404(section_id), data))


@blueprint.put("/settings/<section_id>")
def update_section(section_id):
    data = current_app.data
    data.logger.resetIdler()
    section_id = section_id_or_404(section_id)
    body = request.get_json(silent=True)
    values = body.get("values") if isinstance(body, dict) else None
    if not isinstance(values, dict):
        return jsonify({"error": 'Expected {"values": {key: value}}'}), 400
    described = section_payload(section_id, data)["settings"]
    errors = validate(described, values, custom_enabled(data), data.comPorts)
    if errors:
        return jsonify({"errors": errors}), 400
    form = {
        setting["key"]: form_value(setting, values.get(setting["key"], setting["value"])) for setting in described
    }
    data.config.updateSettings(SECTIONS[section_id], form)
    return jsonify(section_payload(section_id, data))
