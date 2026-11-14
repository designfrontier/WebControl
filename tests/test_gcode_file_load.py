import pytest


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
