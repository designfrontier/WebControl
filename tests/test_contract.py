"""Checks the Flask app against contract/, the frozen frontend <-> backend spec generated from frontend/src/protocol."""
import base64
import inspect
import json
import os
import re

import pytest
from jsonschema import Draft202012Validator

import main
from Actions.actions import Actions
from Background import UIProcessor as ui_processor_module
from Background.UIProcessor import UIProcessor
from conftest import REPO
from helpers import NAMESPACE, drain, messages

CONTRACT = os.path.join(REPO, "contract")
SKIPPED_METHODS = {"HEAD", "OPTIONS"}

# Messages the backend sends only from paths these tests cannot reach without a controller or a classic client.
UNEXERCISED_MESSAGES = {
    "closeAlertModals": "answers the classic UI's alertModalClosed",
    "showFPSpinner": "sent only with a non-empty gcode or board payload",
}


def load(path):
    with open(os.path.join(CONTRACT, path), encoding="utf-8") as handle:
        return json.load(handle)


INDEX = load("index.json")
MESSAGES = INDEX["socket"]["namespaces"][NAMESPACE]["serverToClient"]["message"]["commands"]


def validate(schema_path, value):
    errors = [error.message for error in Draft202012Validator(load(schema_path)).iter_errors(value)]
    assert errors == [], f"{schema_path}: {errors}"


def template(path):
    return re.sub(r"<(?:[^:>]+:)?[^>]+>|\{[^}]+\}", "{}", path)


def test_the_contract_has_a_whole_number_version():
    with open(os.path.join(CONTRACT, "VERSION"), encoding="utf-8") as handle:
        assert int(handle.read().strip()) == INDEX["version"]


def test_every_listed_endpoint_is_registered(app):
    routes = {
        (method, template(rule.rule))
        for rule in app.url_map.iter_rules()
        for method in rule.methods - SKIPPED_METHODS
    }
    missing = [f"{e['method']} {e['path']}" for e in INDEX["http"] if (e["method"], template(e["path"])) not in routes]
    assert missing == []


def test_every_listed_socket_event_has_a_handler():
    handlers = main.socketio.server.handlers
    missing = [
        f"{namespace} {event}"
        for namespace, spec in INDEX["socket"]["namespaces"].items()
        for event in ["connect", *spec["clientToServer"], *spec.get("classicOnlyClientToServer", [])]
        if event not in handlers.get(namespace, {})
    ]
    assert missing == []


def process_action_commands():
    """(command, allowedDuring) in the order processAction tests them, split by its two uploadFlag gates."""
    phase, found = "always", []
    for line in inspect.getsource(Actions.processAction).splitlines():
        if "self.data.uploadFlag == 1" in line:
            phase = "idleOrPaused"
        elif "self.data.uploadFlag > 1" in line:
            phase = "idle"
        elif not line.strip().startswith("#"):
            found += [(command, phase) for command in re.findall(r'msg\["data"\]\["command"\] == "(\w+)"', line)]
    return found


def test_process_action_handles_exactly_the_listed_actions_with_their_run_state_gates():
    listed = {command: spec["allowedDuring"] for command, spec in INDEX["actions"].items()}
    assert dict(process_action_commands()) == listed


def test_each_action_schema_names_its_command():
    for command, spec in INDEX["actions"].items():
        assert load(spec["schema"])["properties"]["data"]["properties"]["command"]["const"] == command


def endpoint(method, path):
    return next(e for e in INDEX["http"] if e["method"] == method and e["path"] == path)


def response_schema(method, path, status):
    return endpoint(method, path)["responses"][str(status)]


def section_ids():
    return load(endpoint("GET", "/api/settings/{sectionId}")["params"])["properties"]["sectionId"]["enum"]


def plain_api_gets():
    return [e["path"] for e in INDEX["http"] if e["method"] == "GET" and e["family"] == "api" and "{" not in e["path"]]


@pytest.mark.parametrize(
    "path", plain_api_gets() + [f"/api/settings/{section}" for section in section_ids()]
)
def test_get_responses_match_their_schemas(client, data, path):
    response = client.get(path)
    assert response.status_code == 200
    contract_path = "/api/settings/{sectionId}" if path.startswith("/api/settings/") else path
    validate(response_schema("GET", contract_path, 200)["schema"], response.get_json())


REFUSALS = [
    ("PUT", "/api/settings/maslow", "/api/settings/{sectionId}", {"values": {"noSuchSetting": "1"}}, 400),
    ("PUT", "/api/settings/maslow", "/api/settings/{sectionId}", {"oops": 1}, 400),
    ("POST", "/api/files/gcode/directories", None, {"name": "  "}, 400),
    ("POST", "/api/boards/directories", None, {"name": ""}, 400),
    ("POST", "/api/machine/quick-configure", None, {"kinematicsType": "Ring"}, 400),
    ("POST", "/api/calibration/triangular", None, {"cut12": "x"}, 400),
    ("POST", "/api/calibration/holey", None, {"measurements": [1, 2]}, 400),
    ("POST", "/api/calibration/triangular/accept", None, {}, 409),
    ("POST", "/api/calibration/holey/accept", None, {}, 409),
    ("PUT", "/api/calibration/optical/settings", None, {"markerX": 1}, 400),
    ("POST", "/api/system/pid-test", None, {"kind": "torque"}, 400),
]


@pytest.mark.parametrize("method, path, contract_path, body, status", REFUSALS)
def test_api_refusals_match_their_schemas(client, data, method, path, contract_path, body, status):
    data.triangularCalibration.hasResult = False
    data.holeyCalibration.hasResult = False
    response = client.open(path, method=method, json=body)
    assert response.status_code == status
    validate(response_schema(method, contract_path or path, status)["schema"], response.get_json())


def test_missing_help_pages_and_settings_sections_answer_as_listed(client, data):
    help_page = client.get("/api/system/help?path=nope.md")
    assert help_page.status_code == 404
    validate(response_schema("GET", "/api/system/help", 404)["schema"], help_page.get_json())
    section = client.get("/api/settings/nope")
    assert section.status_code == 404
    assert response_schema("GET", "/api/settings/{sectionId}", 404)["contentType"] == "text/html"
    assert section.mimetype == "text/html"


def test_classic_routes_answer_as_listed(app, client, data, monkeypatch):
    # app.py sets debug, which turns a missing form field into a raised KeyError; main.py clears it in production.
    monkeypatch.setitem(app.config, "TRAP_BAD_REQUEST_ERRORS", False)
    outside = client.post("/openGCode", data={"selectedGCode": "../../etc/passwd"})
    assert outside.status_code == 400
    validate(response_schema("POST", "/openGCode", 400)["schema"], outside.get_json())

    data.uploadFlag = 1
    locked = client.post("/editGCode", data={"gcode": "G0 X1"})
    assert locked.status_code == 409
    validate(response_schema("POST", "/editGCode", 409)["schema"], locked.get_json())

    missing = client.post("/sendGCode", data={})
    assert missing.status_code == 400
    assert response_schema("POST", "/sendGCode", 400)["contentType"] == "text/html"


def decoded(envelope):
    """The value a command schema describes: JSON text parsed, binary attachments as the base64 the contract uses."""
    validate(INDEX["socket"]["namespaces"][NAMESPACE]["serverToClient"]["message"]["envelope"], {
        **envelope,
        "data": None if isinstance(envelope["data"], (bytes, bytearray)) else envelope["data"],
    })
    value = envelope["data"]
    if envelope.get("dataFormat") == "json":
        return json.loads(value)
    if isinstance(value, (bytes, bytearray)):
        return base64.b64encode(value).decode("ascii")
    return value


@pytest.fixture
def ui(app, data, monkeypatch):
    keys = ("xval_prev", "yval_prev", "zval_prev", "time_prev", "velocity_prev", "leftError_prev", "rightError_prev")
    saved = {key: getattr(data, key) for key in keys}
    emitted = []
    monkeypatch.setattr(
        ui_processor_module.socketio, "emit", lambda event, payload=None, **kwargs: emitted.append((event, payload))
    )
    monkeypatch.setattr(ui_processor_module.time, "sleep", lambda seconds: None)
    processor = UIProcessor()
    processor.app = app
    yield processor, emitted
    for key, value in saved.items():
        setattr(data, key, value)


QUEUED = [
    ("Action", "clearAlarm", ""),
    ("Action", "setAsPause", ""),
    ("Action", "setAsResume", ""),
    ("Action", "updatePorts", ""),
    ("Action", "closeModals", "Notification:"),
    ("Action", "homePositionMessage", {"xval": "12.5", "yval": -3}),
    ("Action", "gcodePositionMessage", {"xval": 1.0, "yval": 2.0, "zval": 0.0, "gcodeLine": "G0 X1", "gcodeLineIndex": 3}),
    ("Action", "connectionStatus", {"status": "disconnected", "port": "none", "fakeServoStatus": False}),
    ("Action", "updateDirectories", {"directory": "jobs"}),
    ("Action", "updateCamera", "on"),
    ("Action", "updatePIDData", json.dumps({"result": "velocity", "version": "2", "data": ["1,2,3"]})),
    ("Action", "updateOpticalCalibrationCurve", {"curveX": [0.0] * 6, "curveY": [1.0] * 6}),
    ("Action", "updateOpticalCalibrationError", {"errorX": [[0.5]], "errorY": [[0.25]]}),
    ("Action", "updateOpticalCalibrationFindCenter", {"opticalCenterX": 1.5, "opticalCenterY": 2.5}),
    ("Action", "pyinstallUpdate", "on"),
    ("Action", "unitsUpdate", ""),
    ("Action", "distToMoveUpdate", ""),
    ("Action", "unitsUpdateZ", ""),
    ("Action", "distToMoveUpdateZ", ""),
    ("Action", "gcodeUpdate", ""),
    ("Action", "boardUpdate", ""),
    ("TextMessage", "", "Connected on port /dev/ttyACM0"),
    ("Alert", "Alert", "Error with initiating move."),
    ("SpinnerMessage", "", "Stock Firmware Update in Progress, Please Wait."),
]


def test_ui_processor_messages_match_their_schemas(ui, data, socket_client):
    processor, emitted = ui
    for item in QUEUED:
        data.ui_queue1.put(*item)
    for raw in drain(data.ui_queue1):
        processor.processMessage(raw)
    processor.lastHealthCheck = 0
    processor.performHealthCheck()
    processor.setPosOnScreen("<Idle,MPos:1.0,2.0,3.0,WPos:0.0,0.0,0.0>\r\n")
    data.leftError_prev = data.rightError_prev = 0
    processor.setErrorOnScreen("[PE:1.5,-2.5,127]\r\n")
    processor.sendAlarm("Alarm: Position Lost.  Reset Chains.")
    processor.sendControllerMessage("Grbl 1.1\r\n")
    processor.sendCameraMessage("cameraImageUpdated", "aGVsbG8=")
    processor.sendCalibrationImage("OpticalCalibrationImageUpdated", "aGVsbG8=")

    socket_client.emit("settingRequest", {"data": {"section": "Computed Settings", "setting": "units"}}, namespace=NAMESPACE)
    received = [{"command": m["command"], "data": m["data"]} for m in messages(socket_client)]
    envelopes = [{"command": c, "data": decoded(p)} for event, p in emitted if event == "message" for c in [p["command"]]]

    sent = envelopes + received
    assert [m["command"] for m in sent if m["command"] not in MESSAGES] == []
    for message in sent:
        validate(MESSAGES[message["command"]]["schema"], message["data"])
    unexercised = set(MESSAGES) - {m["command"] for m in sent} - set(UNEXERCISED_MESSAGES)
    assert unexercised == set()
