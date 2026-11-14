import base64
import json

from flask import Blueprint, current_app, jsonify

blueprint = Blueprint("viewer", __name__)

MM_PER_INCH = 25.4
MACHINE_SETTINGS = ("bedWidth", "bedHeight", "motorOffsetY", "motorSpacingX")


def _inches(value):
    try:
        return round(float(value) / MM_PER_INCH, 4)
    except (TypeError, ValueError):
        return None


def _base64(blob):
    """Gcode and cut data are gzip bytes, or None/"" when nothing is loaded."""
    return base64.b64encode(blob).decode("ascii") if isinstance(blob, (bytes, bytearray)) and blob else None


def _board(board):
    try:
        return json.loads(board.getBoardInfoJSON())
    except (AttributeError, TypeError, ValueError):
        return None


def viewer_snapshot(data):
    """Everything the 3D view draws, in inches, for one client: the socket pushes the same data only as broadcasts."""
    machine = {key: _inches(data.config.getValue("Maslow Settings", key)) for key in MACHINE_SETTINGS}
    board = data.boardManager.getCurrentBoard()
    return {
        "machine": machine,
        "gcode": _base64(data.compressedGCode3D),
        "board": _board(board),
        "cutData": _base64(board.getCompressedCutData()),
    }


@blueprint.get("/viewer")
def get_viewer():
    current_app.data.logger.resetIdler()
    return jsonify(viewer_snapshot(current_app.data))
