from flask import Blueprint, current_app, jsonify

blueprint = Blueprint("state", __name__)


def _number(value):
    try:
        return float(value)
    except (TypeError, ValueError):
        return 0.0


def startup_state(data):
    """The computed settings the old UI fetched one settingRequest at a time on connect."""
    config = data.config
    return {
        "units": config.getValue("Computed Settings", "units"),
        "distToMove": _number(config.getValue("Computed Settings", "distToMove")),
        "unitsZ": config.getValue("Computed Settings", "unitsZ"),
        "distToMoveZ": _number(config.getValue("Computed Settings", "distToMoveZ")),
        "homePosition": {
            "x": _number(config.getValue("Advanced Settings", "homeX")),
            "y": _number(config.getValue("Advanced Settings", "homeY")),
        },
        "pauseButtonSetting": "Pause" if data.uploadFlag in (0, 1) else "Resume",
        "hostAddress": data.hostAddress,
        "controllerFirmwareVersion": _number(data.controllerFirmwareVersion),
        "macros": [
            {"command": f"macro{n}", "title": str(config.getValue("Maslow Settings", f"macro{n}_title") or "")}
            for n in (1, 2)
        ],
    }


@blueprint.get("/state")
def get_state():
    current_app.data.logger.resetIdler()
    return jsonify(startup_state(current_app.data))
