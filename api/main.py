from flask import Blueprint, current_app, jsonify

blueprint = Blueprint("main", __name__)


def _flag(value):
    try:
        return bool(int(float(value)))
    except (TypeError, ValueError):
        return str(value).strip().lower() == "true"


def _gcode_position(data):
    lines = data.gcode or []
    index = data.gcodeIndex if 0 <= data.gcodeIndex < len(lines) else 0
    return {"lineIndex": index, "line": lines[index] if lines else "", "lineCount": len(lines)}


def main_screen_state(data):
    """What the control screen needs at once on (re)connect instead of waiting for the next 5 s status push."""
    return {
        "status": {
            "uploadFlag": data.uploadFlag,
            "positioningMode": data.positioningMode,
            "currentTool": data.currentTool,
        },
        "touchPlate": _flag(data.config.getValue("Advanced Settings", "touchPlate")),
        "gcode": _gcode_position(data),
    }


@blueprint.get("/main")
def get_main_screen_state():
    current_app.data.logger.resetIdler()
    return jsonify(main_screen_state(current_app.data))
