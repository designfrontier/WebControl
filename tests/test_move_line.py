import re

import pytest

NUMBER = re.compile(r"([XY])(-?\d+(?:\.(\d+))?)")


@pytest.fixture
def gcode_file(data):
    gcodeFile = data.gcodeFile
    saved = (list(data.gcodeShift), gcodeFile.truncate)
    yield gcodeFile
    data.gcodeShift, gcodeFile.truncate = saved


def shifted(gcodeFile, line, shift, truncate=-1):
    gcodeFile.data.gcodeShift = list(shift)
    gcodeFile.truncate = truncate
    return gcodeFile.moveLine(line)


def test_the_users_line_is_sent_with_its_own_precision(gcode_file):
    assert shifted(gcode_file, "G1 X-137.5730 Y41.3110 Z-9.529", (41.6763, 60.1447)) == "G1 X-95.8967 Y101.4557 Z-9.529 "


@pytest.mark.parametrize("home", [(41.6763, 60.1447), (0.1, 0.2), (-12.3456, 7.89), (123.4567, -45.6789)])
def test_no_shifted_number_carries_binary_rounding_digits(gcode_file, home):
    lines = [f"G1 X{x / 10000:.4f} Y{y / 1000:.3f}" for x in range(-2000000, 2000000, 7919) for y in (-1234, 5, 98765)]
    decimals = [
        len(match.group(3) or "")
        for line in lines
        for match in NUMBER.finditer(shifted(gcode_file, line, home))
    ]
    assert max(decimals) <= 4


def test_a_shifted_value_is_exact_in_decimal(gcode_file):
    assert shifted(gcode_file, "G0 X10.1 Y0.2", (0.2, 0.1)) == "G0 X10.3 Y0.3 "


def test_whole_numbers_stay_whole_with_no_shift(gcode_file):
    assert shifted(gcode_file, "G0 X10 Y-5", (0.0, 0.0)) == "G0 X10 Y-5 "


def test_the_truncate_option_still_caps_the_digits(gcode_file):
    assert shifted(gcode_file, "G1 X-137.57301 Y41.31109", (41.6763, 60.1447), truncate=2) == "G1 X-95.90 Y101.46 "


def test_more_than_six_decimals_are_capped(gcode_file):
    assert shifted(gcode_file, "G1 X1.123456789", (0.0, 0.0)) == "G1 X1.123457 "


def test_zero_is_never_written_negative(gcode_file):
    assert shifted(gcode_file, "G1 X-41.6763", (41.6763, 0.0)) == "G1 X0.0000 "


def test_comments_and_lines_without_coordinates_are_kept(gcode_file):
    assert shifted(gcode_file, "G1 X1.5 (move) ; note", (1.0, 0.0)) == "G1 X2.5  (move) ; note"
    assert shifted(gcode_file, "(only a comment)", (1.0, 0.0)) == "(only a comment)"
    assert shifted(gcode_file, "G1 Z-1.25 F300", (1.0, 1.0)) == "G1 Z-1.25 F300 "


def test_a_spaced_coordinate_is_shifted(gcode_file):
    assert shifted(gcode_file, "G1 X 10.5", (1.25, 0.0)) == "G1 X11.75 "


def test_an_unreadable_coordinate_leaves_the_line_unchanged(gcode_file):
    assert shifted(gcode_file, "G1 X. Y2", (1.0, 1.0)) == "G1 X. Y2"
