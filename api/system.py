"""System endpoints for the new frontend: version and capability info, PID tuning, releases and offline help."""
import os
import re

import frontmatter
from flask import Blueprint, abort, current_app, jsonify, request, send_from_directory

blueprint = Blueprint("system", __name__, url_prefix="/system")

MOTORS = ("L", "R", "M", "N")
PID_FIELDS = {
    "velocity": ("start", "stop", "steps", "version", "kp", "ki", "kd"),
    "position": ("start", "stop", "steps", "time", "version", "kp", "ki", "kd"),
}
# The old pidTuning.js built these dicts; Actions.velocityPIDTest/positionPIDTest concatenate them into B13/B14.
PID_ACTION_KEYS = {
    "velocity": ("executeVelocityPIDTest", "v", {"kp": "KpV", "ki": "KiV", "kd": "KdV"}),
    "position": ("executePositionPIDTest", "p", {"kp": "KpP", "ki": "KiP", "kd": "KdP"}),
}


def _text(value):
    return None if value is None else str(value)


def _firmware_version(data):
    try:
        return float(data.controllerFirmwareVersion)
    except (TypeError, ValueError):
        return 0.0


def _release_tag(release):
    return (getattr(release, "tag_name", None) or _text(release)) if release else None


def system_info(data):
    """What the old actions/about templates were rendered with."""
    firmware = _firmware_version(data)
    return {
        "version": str(data.pyInstallCurrentVersion),
        "platform": data.platform,
        "controllerFirmwareVersion": firmware,
        "firmware": {
            "stock": _text(data.stockFirmwareVersion),
            "holey": _text(data.holeyFirmwareVersion),
            "custom": _text(data.customFirmwareVersion),
        },
        "canTurnOff": data.platform in ("RPI", "PYINSTALLER"),
        "fourMotor": firmware >= 100,
        "update": {
            "available": bool(data.pyInstallUpdateAvailable),
            "version": _release_tag(data.pyInstallUpdateVersion) if data.pyInstallUpdateAvailable else None,
        },
    }


@blueprint.get("/info")
def get_info():
    current_app.data.logger.resetIdler()
    return jsonify(system_info(current_app.data))


def pid_settings(data):
    def value(key):
        return _text(data.config.getValue("Advanced Settings", key)) or ""

    return {
        "velocity": {"kp": value("KpV"), "ki": value("KiV"), "kd": value("KdV")},
        "position": {"kp": value("KpPos"), "ki": value("KiPos"), "kd": value("KdPos")},
        "fourMotor": _firmware_version(data) >= 100,
    }


@blueprint.get("/pid")
def get_pid():
    current_app.data.logger.resetIdler()
    return jsonify(pid_settings(current_app.data))


PLAIN_DECIMAL = re.compile(r"-?(\d+\.?\d*|\.\d+)")


def _is_number(value):
    """Plain decimals only: the firmware's parser knows no exponents, nan, inf or digit separators."""
    return not isinstance(value, bool) and bool(PLAIN_DECIMAL.fullmatch(str(value).strip()))


def pid_test_action(body):
    """Validates a PID test request and builds the processAction message the old UI sent, or raises ValueError."""
    kind = body.get("kind")
    if kind not in PID_FIELDS:
        raise ValueError("kind must be velocity or position")
    if body.get("motor") not in MOTORS:
        raise ValueError("motor must be one of " + ", ".join(MOTORS))
    bad = [field for field in PID_FIELDS[kind] if not _is_number(body.get(field))]
    if bad:
        raise ValueError("not numbers: " + ", ".join(bad))
    command, prefix, gains = PID_ACTION_KEYS[kind]
    arg = {prefix + "Motor": body["motor"]}
    for field in PID_FIELDS[kind]:
        key = gains.get(field) or prefix + field.capitalize()
        arg[key] = str(body[field]).strip()
    return {"data": {"command": command, "arg": arg, "arg1": ""}}


@blueprint.post("/pid-test")
def post_pid_test():
    data = current_app.data
    data.logger.resetIdler()
    try:
        message = pid_test_action(request.get_json(silent=True) or {})
    except ValueError as error:
        return jsonify({"error": str(error)}), 400
    if data.uploadFlag != 0:
        return jsonify({"error": "Stop the running gcode before starting a PID test."}), 409
    data.actions.processAction(message)
    return jsonify({"status": "sent"}), 202


def releases_payload(data):
    manager = data.releaseManager
    releases = manager.getReleases() if manager.releases is not None else []
    latest = manager.getLatestRelease()
    return {
        "current": "v" + str(data.pyInstallCurrentVersion),
        "latest": _release_tag(latest),
        "releases": [
            {"tag": release.tag_name, "title": release.title or release.tag_name, "body": release.body or ""}
            for release in releases
        ],
    }


@blueprint.get("/releases")
def get_releases():
    current_app.data.logger.resetIdler()
    return jsonify(releases_payload(current_app.data))


def docs_root(data):
    base = data.platformHome if data.platform == "PYINSTALLER" else os.path.dirname(os.path.dirname(__file__))
    return os.path.realpath(os.path.join(base, "docs"))


def clean_markdown(text):
    """Turns the Jekyll/HTML bits of docs/*.md into plain markdown the client renders without raw HTML."""
    text = re.sub(r"\{\{\s*site\.baseurl\s*\}\}\{%\s*link\s+(\S+?)\s*%\}", r"\1", text)
    text = text.replace("{: .label .label-blue }", "").replace("Â", "")
    text = re.sub(r"</br>", "  ", text)
    text = re.sub(r"</?span[^>]*>", "", text)
    return re.sub(
        r'<img\s+src="([^"]+)"\s+alt="([^"]*)"[^>]*>',
        lambda match: "![{}]({})".format(match.group(2), match.group(1)),
        text,
    )


def resolve_doc(root, path):
    full = os.path.realpath(os.path.join(root, path))
    inside = full.startswith(root + os.sep)
    return full if inside and full.endswith(".md") and os.path.isfile(full) else None


@blueprint.get("/help")
def get_help():
    data = current_app.data
    data.logger.resetIdler()
    path = request.args.get("path") or "index.md"
    full = resolve_doc(docs_root(data), path)
    if full is None:
        return jsonify({"error": "no such help page"}), 404
    with open(full, encoding="utf-8") as handle:
        page = frontmatter.loads(handle.read())
    title = page.get("title") or os.path.splitext(os.path.basename(path))[0]
    return jsonify({"path": path, "title": str(title), "markdown": clean_markdown(page.content)})


@blueprint.get("/help/assets/<path:path>")
def get_help_asset(path):
    root = os.path.join(docs_root(current_app.data), "assets")
    if not os.path.isfile(os.path.join(root, path)):
        abort(404)
    return send_from_directory(root, path)
