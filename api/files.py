"""JSON for the gcode library and the loaded gcode, replacing the HTML the openGCode/saveGCode/uploadGCode pages built.

Opening, uploading, saving, editing and sending gcode still go through the classic form routes in main.py.
"""
import os

from flask import Blueprint, current_app, jsonify, request
from werkzeug.utils import secure_filename

from File.safePaths import UnsafePathError, resolveInside

blueprint = Blueprint("files", __name__)

ROOT = "."


def library_root(data, kind):
    return os.path.join(data.config.getHome(), ".WebControl", kind)


def directory_setting(value):
    """The classic UI stores the root directory as "./" or "."; subdirectories may carry a trailing slash."""
    return str(value or "").rstrip("/") or ROOT


def _visible(names):
    return sorted((name for name in names if not name.startswith(".")), key=str.lower)


def _files_in(path):
    try:
        return _visible(name for name in os.listdir(path) if os.path.isfile(os.path.join(path, name)))
    except OSError:
        return []


def list_library(root):
    """Files directly in `root` (directory ".") and in each of its immediate subdirectories."""
    try:
        subdirectories = _visible(entry.name for entry in os.scandir(root) if entry.is_dir())
    except OSError:
        subdirectories = []
    directories = [ROOT, *subdirectories]
    return {
        "directories": directories,
        "files": [
            {"directory": directory, "name": name}
            for directory in directories
            for name in _files_in(root if directory == ROOT else os.path.join(root, directory))
        ],
    }


def create_directory(root):
    """Creates the subdirectory named in the JSON body, sanitised to one path segment; returns (response, status)."""
    body = request.get_json(silent=True) or {}
    name = secure_filename(str(body.get("name", "")).strip())
    if not name:
        return jsonify({"error": "Enter a directory name using letters, numbers, dashes or underscores."}), 400
    try:
        os.makedirs(resolveInside(root, name), exist_ok=True)
    except UnsafePathError:
        return jsonify({"error": "That directory points outside the library."}), 400
    return jsonify({"directory": name, **list_library(root)}), 201


def _gcode_lines(data):
    return data.gcode if isinstance(data.gcode, list) else []


def loaded_gcode(data):
    """Where the loaded gcode came from, relative to the library, and how many lines it has."""
    lines = _gcode_lines(data)
    filename = data.gcodeFile.filename or ""
    if not lines or not filename:
        return {"directory": None, "name": None, "lineCount": len(lines)}
    relative = os.path.relpath(filename, library_root(data, "gcode"))
    directory = os.path.dirname(relative)
    inside = not relative.startswith("..") and os.sep not in directory and "/" not in directory
    return {
        "directory": (directory or ROOT) if inside else None,
        "name": os.path.basename(filename),
        "lineCount": len(lines),
    }


@blueprint.get("/files/gcode")
def get_gcode_library():
    data = current_app.data
    data.logger.resetIdler()
    config = data.config
    return jsonify(
        {
            **list_library(library_root(data, "gcode")),
            "lastDirectory": directory_setting(config.getValue("Computed Settings", "lastSelectedDirectory")),
            "lastFile": str(config.getValue("Maslow Settings", "openFile") or ""),
            "validExtensions": str(config.getValue("WebControl Settings", "validExtensions") or ""),
        }
    )


@blueprint.post("/files/gcode/directories")
def post_gcode_directory():
    data = current_app.data
    data.logger.resetIdler()
    response, status = create_directory(library_root(data, "gcode"))
    if status == 201:
        data.ui_queue1.put("Action", "updateDirectories", {"directory": response.get_json()["directory"]})
    return response, status


@blueprint.get("/files/gcode/loaded")
def get_loaded_gcode():
    current_app.data.logger.resetIdler()
    return jsonify(loaded_gcode(current_app.data))


@blueprint.get("/files/gcode/text")
def get_gcode_text():
    data = current_app.data
    data.logger.resetIdler()
    return jsonify({"text": "\n".join(_gcode_lines(data)).rstrip()})


@blueprint.get("/files/gcode/custom")
def get_custom_gcode():
    data = current_app.data
    data.logger.resetIdler()
    return jsonify({"text": data.sentCustomGCode or ""})
