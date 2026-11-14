"""Machine setup data for the new UI: what the old Z-axis, sprocket and quick-configure modals got from Jinja."""
import math

from flask import Blueprint, current_app, jsonify, request

blueprint = Blueprint("machine", __name__)

KINEMATICS_TYPES = ("Ring", "Linkage", "Pantograph", "Metal Maslow", "Custom", "Quadrilateral")
CHAIN_OVER_SPROCKET = ("Top", "Bottom")
RING_RADIUS = 138.4
QUICK_CONFIGURE_NUMBERS = ("rotationRadius", "motorSpacingX", "motorOffsetY")


def _number(value):
    try:
        number = float(value)
    except (TypeError, ValueError):
        return 0.0
    return number if math.isfinite(number) else 0.0


def _firmware_version(data):
    return _number(data.controllerFirmwareVersion)


def machine_setup(data):
    config = data.config
    version = _firmware_version(data)
    return {
        "chainExtendLength": _number(config.getValue("Advanced Settings", "chainExtendLength")),
        "fourMotor": version >= 100,
        "firmwareSupportsZaxisLimit": version > 1.26,
        "touchPlate": bool(config.getValue("Advanced Settings", "touchPlate")),
        "controllerFirmwareVersion": version,
    }


def quick_configure(config):
    """Mirrors WebPageProcessor's quickConfigure page: triangular kits read back as Ring or Custom."""
    rotation_radius = _number(config.getValue("Advanced Settings", "rotationRadius"))
    kinematics = config.getValue("Advanced Settings", "kinematicsType")
    if kinematics != "Quadrilateral":
        kinematics = "Ring" if abs(rotation_radius - RING_RADIUS) < 0.01 else "Custom"
    chain_over = config.getValue("Advanced Settings", "chainOverSprocket")
    return {
        "kinematicsType": kinematics,
        "rotationRadius": rotation_radius,
        "chainOverSprocket": chain_over if chain_over in CHAIN_OVER_SPROCKET else "Top",
        "motorSpacingX": _number(config.getValue("Maslow Settings", "motorSpacingX")),
        "motorOffsetY": _number(config.getValue("Maslow Settings", "motorOffsetY")),
    }


def _form_text(number):
    """The old form posted the typed text; send whole numbers without a trailing .0 like it did."""
    return str(int(number)) if number.is_integer() else str(number)


def parse_quick_configure(body):
    """Returns (form, None) for a valid request body, or (None, error message)."""
    if not isinstance(body, dict):
        return None, "Expected a JSON object"
    if body.get("kinematicsType") not in KINEMATICS_TYPES:
        return None, "kinematicsType must be one of " + ", ".join(KINEMATICS_TYPES)
    if body.get("chainOverSprocket") not in CHAIN_OVER_SPROCKET:
        return None, "chainOverSprocket must be Top or Bottom"
    form = {"kinematicsType": body["kinematicsType"], "chainOverSprocket": body["chainOverSprocket"]}
    for key in QUICK_CONFIGURE_NUMBERS:
        value = body.get(key)
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
            return None, key + " must be a number of mm, zero or more"
        form[key] = _form_text(float(value))
    return form, None


@blueprint.get("/machine/setup")
def get_setup():
    current_app.data.logger.resetIdler()
    return jsonify(machine_setup(current_app.data))


@blueprint.get("/machine/quick-configure")
def get_quick_configure():
    current_app.data.logger.resetIdler()
    return jsonify(quick_configure(current_app.data.config))


@blueprint.post("/machine/quick-configure")
def post_quick_configure():
    current_app.data.logger.resetIdler()
    form, error = parse_quick_configure(request.get_json(silent=True))
    if error:
        return jsonify({"error": error}), 400
    current_app.data.config.updateQuickConfigure(form)
    return jsonify(quick_configure(current_app.data.config))
