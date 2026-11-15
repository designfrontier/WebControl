from flask import Blueprint, current_app, jsonify

from DataStructures.data import pauseButtonLabel
from Connection import firmwareFacts

blueprint = Blueprint("state", __name__)


def _number(value):
    try:
        return float(value)
    except (TypeError, ValueError):
        return 0.0


def job_status(data):
    """Whether a job is running or paused (uploadFlag drops to 0 on controller pauses), and whether it waits on a
    manual Z adjustment such as a tool change."""
    return {
        "jobInProgress": bool(data.jobInProgress),
        "manualZAxisAdjust": bool(data.manualZAxisAdjust),
    }


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
        "pauseButtonSetting": pauseButtonLabel(data),
        **job_status(data),
        "hostAddress": data.hostAddress,
        "controllerFirmwareVersion": _number(data.controllerFirmwareVersion),
        "controllerBoard": data.controllerBoard,
        "firmwareBuild": firmwareFacts.firmwareBuild(data),
        "macros": [
            {"command": f"macro{n}", "title": str(config.getValue("Maslow Settings", f"macro{n}_title") or "")}
            for n in (1, 2)
        ],
    }


@blueprint.get("/state")
def get_state():
    current_app.data.logger.resetIdler()
    return jsonify(startup_state(current_app.data))
