"""JSON for the board library and the current board, replacing the HTML the openBoard/saveBoard/editBoard pages built.

Opening, saving, editing and trimming boards still go through the classic form routes in main.py.
"""
from flask import Blueprint, current_app, jsonify

from api.files import create_directory, directory_setting, library_root, list_library

blueprint = Blueprint("boards", __name__)

MM_PER_INCH = 25.4


def current_board(data):
    """The current board in the machine's units; boards are stored in inches, as the editBoard page converted them."""
    board = data.boardManager.getCurrentBoard()
    metric = data.units == "MM"
    scale = MM_PER_INCH if metric else 1

    def scaled(value):
        return round(float(value) * scale, 2)

    return {
        "boardID": str(board.boardID),
        "material": str(board.material),
        "width": scaled(board.width),
        "height": scaled(board.height),
        "thickness": scaled(board.thickness),
        "centerX": scaled(board.centerX),
        "centerY": scaled(board.centerY),
        "units": "mm" if metric else "inches",
        "fileName": str(board.boardFilename or ""),
        "router": {"x": float(data.xval), "y": float(data.yval)},
    }


@blueprint.get("/boards/files")
def get_board_library():
    data = current_app.data
    data.logger.resetIdler()
    config = data.config
    current_file = data.boardManager.getCurrentBoard().boardFilename
    return jsonify(
        {
            **list_library(library_root(data, "boards")),
            "lastDirectory": directory_setting(config.getValue("Computed Settings", "lastSelectedBoardDirectory")),
            "lastFile": str(current_file or config.getValue("Maslow Settings", "openBoardFile") or ""),
        }
    )


@blueprint.post("/boards/directories")
def post_board_directory():
    current_app.data.logger.resetIdler()
    return create_directory(library_root(current_app.data, "boards"))


@blueprint.get("/boards/current")
def get_current_board():
    current_app.data.logger.resetIdler()
    return jsonify(current_board(current_app.data))
