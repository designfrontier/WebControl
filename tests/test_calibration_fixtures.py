"""Checks the calibration code still reproduces tests/fixtures/calibration; see the README there to regenerate."""
import copy
import json
import os

import pytest

import calibration_fixture_cases as cases
from helpers import drain

FIXTURES = os.path.join(os.path.dirname(__file__), "fixtures", "calibration")
REGENERATE = os.environ.get("REGENERATE_CALIBRATION_FIXTURES") == "1"


@pytest.fixture
def geometry(data):
    saved = copy.deepcopy(data.config.settings)
    simulator = dict(vars(data.holeyKinematics))
    cases.apply_geometry(data)
    yield data
    data.config.settings = saved
    vars(data.holeyKinematics).clear()
    vars(data.holeyKinematics).update(simulator)
    data.holeyCalibration.hasResult = False
    data.triangularCalibration.hasResult = False
    for queue in (data.gcode_queue, data.message_queue):
        drain(queue)


def matches(actual, expected, tolerance):
    if isinstance(expected, dict):
        return actual.keys() == expected.keys() and all(matches(actual[k], v, tolerance) for k, v in expected.items())
    if isinstance(expected, list):
        return len(actual) == len(expected) and all(matches(a, e, tolerance) for a, e in zip(actual, expected))
    if isinstance(expected, float) and not isinstance(actual, bool):
        return actual == pytest.approx(expected, abs=tolerance, rel=0)
    return actual == expected


def check(name, actual):
    path = os.path.join(FIXTURES, name)
    normalised = json.loads(json.dumps(actual))
    if REGENERATE:
        with open(path, "w") as outfile:
            json.dump(normalised, outfile, indent=2)
            outfile.write("\n")
    with open(path) as infile:
        expected = json.load(infile)
    assert matches(normalised, expected, expected.get("tolerance", 0))


def test_holey_calibrate_reproduces_the_fixture(geometry):
    check("holey_calibrate.json", cases.holey_cases(geometry))


def test_holey_kinematics_reproduces_the_fixture():
    check("kinematics.json", cases.kinematics_cases())


def test_triangular_calculate_reproduces_the_fixture(geometry):
    check("triangular_calculate.json", cases.triangular_cases(geometry))


def test_accept_queues_the_fixture_commands(geometry):
    check("accept_commands.json", cases.accept_cases(geometry))


def test_the_fixtures_cover_refusals_and_noise():
    with open(os.path.join(FIXTURES, "holey_calibrate.json")) as infile:
        holey = json.load(infile)["cases"]
    assert sum(case["expected"] is None for case in holey) >= 1
    assert sum("noise" in case["name"] for case in holey) >= 3
    assert all(case["expected"]["totalError"] < 1e-6 for case in holey if case["name"].startswith(("ideal", "exact")))
