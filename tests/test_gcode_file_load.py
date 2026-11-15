import pytest

from helpers import decode_ui_message, drain


@pytest.fixture
def loaded_gcode(data):
    saved = (data.gcode, data.gcodeIndex, data.gcodeFile.filename)
    yield
    data.gcode, data.gcodeIndex, data.gcodeFile.filename = saved


def test_opening_a_shorter_file_after_scrubbing_starts_it_from_the_top(data, loaded_gcode, tmp_path):
    data.gcode = [f"G0 X{n} " for n in range(100)]
    data.gcodeIndex = 80
    short = tmp_path / "short.nc"
    short.write_text("G0 X1\nG0 X2\n")
    data.gcodeFile.filename = str(short)

    assert data.gcodeFile.loadUpdateFile() is True

    assert data.gcodeIndex == 0


def test_editing_the_gcode_shorter_keeps_the_index_inside_it(data, loaded_gcode):
    data.gcode = [f"G0 X{n} " for n in range(100)]
    data.gcodeIndex = 80

    data.gcodeFile.loadUpdateFile("G0 X1\nG0 X2\nG0 X3")

    assert data.gcodeIndex == len(data.gcode) - 1


def test_the_preview_skips_a_word_it_cannot_read_and_draws_the_rest(data, loaded_gcode):
    [drain(queue) for queue in (data.console_queue, data.ui_queue1)]

    assert data.gcodeFile.loadUpdateFile("G0 X1\nG1 X2 Z`3\nG1 X3 Y4\n") is True

    scale = data.gcodeFile.canvasScaleFactor
    assert data.gcodeFile.line3D[-1].points[-1][:2] == pytest.approx((3 * scale, 4 * scale))
    assert any("Z`3" in line for line in drain(data.console_queue))
    assert "Alert" not in [decode_ui_message(raw)[0] for raw in drain(data.ui_queue1)]
