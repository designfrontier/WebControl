import pytest

from helpers import drain

HOLES = [("0", "355.6"), ("-965.2", "355.6"), ("-965.2", "-355.6"), ("0", "-355.6"), ("965.2", "-355.6"), ("965.2", "355.6")]


def expected(units):
    cuts = [line for x, y in HOLES for line in (f"G0 X{x} Y{y}", "G1 Z-5 F500", "G0 Z5")]
    tail = ["G20"] if units == "INCHES" else []
    return ["G21", "G90", "G40", "G17", "G0 Z5", "M3", *cuts, "G0 X0 Y0", "M5", *tail]


@pytest.mark.parametrize("units", ["MM", "INCHES"])
def test_holey_pattern_raises_the_bit_before_moving_and_plunges_at_a_feed_rate(data, units):
    data.units = units
    assert data.holeyCalibration.CutTestPattern() is True
    assert drain(data.gcode_queue) == expected(units)
