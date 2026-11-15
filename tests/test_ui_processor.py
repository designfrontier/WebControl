import json

import pytest

from Background.UIProcessor import UIProcessor
from helpers import drain

SAVED_DATA = ["gcode", "gcodeIndex", "zval", "xval_prev", "yval_prev", "zval_prev", "time_prev", "velocity_prev"]


class StopLoop(BaseException):
    pass


@pytest.fixture
def ui(app, data):
    saved = {key: getattr(data, key) for key in SAVED_DATA}
    saved_first_run = data.config.firstRun
    processor = UIProcessor()
    processor.app = app
    yield processor
    for key, value in saved.items():
        setattr(data, key, value)
    data.config.firstRun = saved_first_run


def test_a_one_line_file_reports_its_progress(ui, data, monkeypatch):
    sent = []
    monkeypatch.setattr(ui, "sendPositionMessage", sent.append)
    data.gcode = ["G0 X1 "]
    data.gcodeIndex = 0

    ui.setPosOnScreen("<Idle,MPos:1.0,2.0,3.0,WPos:0.0,0.0,0.0>\r\n")

    assert sent[0]["pcom"] == "0.0%"


def test_an_exception_in_one_pass_does_not_end_the_loop(app, ui, data, monkeypatch):
    data.config.firstRun = False
    outcomes = iter([RuntimeError("one bad pass"), StopLoop()])

    def health_check():
        raise next(outcomes)

    monkeypatch.setattr(ui, "performHealthCheck", health_check)

    with pytest.raises(StopLoop):
        ui.start(app)


@pytest.mark.parametrize("version, reported", [(51.27, 51.27), (0, 0.0), ("garbled", 0.0)])
def test_the_health_check_reports_the_controller_firmware_version(ui, data, monkeypatch, version, reported):
    emitted = []
    monkeypatch.setattr("Background.UIProcessor.socketio.emit", lambda *args, **kwargs: emitted.append(args))
    monkeypatch.setattr(ui, "performStatusCheck", lambda *args: None)
    monkeypatch.setattr(data, "controllerFirmwareVersion", version)
    ui.lastHealthCheck = 0

    ui.performHealthCheck()

    health = json.loads(next(payload["data"] for _, payload in emitted if payload["command"] == "healthMessage"))
    assert health["controllerFirmwareVersion"] == reported


def status_messages(emitted):
    return [json.loads(payload["data"]) for _, payload in emitted if payload["command"] == "statusMessage"]


def test_the_status_message_carries_the_job_state_and_goes_out_when_it_changes(ui, data, monkeypatch):
    emitted = []
    monkeypatch.setattr("Background.UIProcessor.socketio.emit", lambda *args, **kwargs: emitted.append(args))
    monkeypatch.setattr(data, "uploadFlag", 0)
    monkeypatch.setattr(data, "jobInProgress", False)
    monkeypatch.setattr(data, "manualZAxisAdjust", False)
    ui.performStatusCheck()
    ui.performStatusCheck()
    assert len(status_messages(emitted)) == 1

    monkeypatch.setattr(data, "jobInProgress", True)
    monkeypatch.setattr(data, "manualZAxisAdjust", True)
    ui.performStatusCheck()

    latest = status_messages(emitted)[-1]
    assert (latest["uploadFlag"], latest["jobInProgress"], latest["manualZAxisAdjust"]) == (0, True, True)


def test_a_buffer_overflow_raises_an_alarm(ui, data, monkeypatch):
    emitted = []
    monkeypatch.setattr("Background.UIProcessor.socketio.emit", lambda *args, **kwargs: emitted.append(args))
    monkeypatch.setattr(ui, "performHealthCheck", lambda: None)
    monkeypatch.setattr(ui, "performStatusCheck", lambda: None)
    data.config.firstRun = False
    drain(data.ui_controller_queue)
    data.ui_controller_queue.put("Buffer overflow!\r\n")

    with ui.app.app_context():
        ui.processPass()

    alarms = [json.loads(payload["data"])["message"] for _, payload in emitted if payload["command"] == "alarm"]
    assert alarms == ["Alarm: Controller Buffer Overflow"]


@pytest.fixture
def error_report(ui, data, monkeypatch):
    settings = {"positionErrorLimit": "2.0", "computedPosition": "0"}
    real_get_value = data.config.getValue
    monkeypatch.setattr(data.config, "getValue", lambda section, key: settings.get(key) or real_get_value(section, key))
    for key in ["leftError", "rightError", "leftError_prev", "rightError_prev", "computedX", "computedY",
                "leftChain", "rightChain", "bufferSize"]:
        monkeypatch.setattr(data, key, getattr(data, key))
    drain(data.console_queue)
    return ui


@pytest.mark.parametrize("version", [0, 1.26, 51.27])
def test_a_holey_error_report_is_parsed_by_its_field_count(error_report, data, monkeypatch, version):
    monkeypatch.setattr(data, "controllerFirmwareVersion", version)

    error_report.setErrorOnScreen("[PE:0.5,0.25,120,1650.5,1700.25]\r\n")

    assert (data.leftError, data.rightError, data.bufferSize) == (0.25, 0.125, 120)
    assert (data.leftChain, data.rightChain) == (1650.5, 1700.25)
    assert drain(data.console_queue) == []


@pytest.mark.parametrize("version", [0, 1.26, 51.27])
def test_a_stock_error_report_is_parsed_by_its_field_count(error_report, data, monkeypatch, version):
    monkeypatch.setattr(data, "controllerFirmwareVersion", version)
    monkeypatch.setattr(data, "leftChain", 1.0)

    error_report.setErrorOnScreen("[PE:0.5,0.25,120]\r\n")

    assert (data.leftError, data.rightError, data.bufferSize, data.leftChain) == (0.25, 0.125, 120, 1.0)
    assert drain(data.console_queue) == []



def test_the_position_message_reports_default_motor_power_and_feed(ui, data, monkeypatch):
    sent = []
    monkeypatch.setattr(ui, "sendPositionMessage", sent.append)
    monkeypatch.setattr(data, "feedRate", 20)

    ui.setPosOnScreen("<Idle,MPos:9.0,8.0,7.0,WPos:0.0,0.0,0.0>\r\n")

    assert {key: sent[0][key] for key in ["lpwr", "rpwr", "zpwr", "rqvel"]} == {"lpwr": 15, "rpwr": 15, "zpwr": 15, "rqvel": 20}
