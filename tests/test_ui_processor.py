import pytest

from Background.UIProcessor import UIProcessor

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
