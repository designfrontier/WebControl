"""Checks the gcode and board code still reproduces tests/fixtures/gcode and tests/fixtures/boards."""
import copy
import json
import os

import pytest

import gcode_fixture_cases as cases
from helpers import drain

FIXTURES = os.path.join(os.path.dirname(__file__), "fixtures")
REGENERATE = os.environ.get("REGENERATE_GCODE_FIXTURES") == "1"
TOLERANCE = 1e-9
GCODE_FILES = sorted(name for name in os.listdir(cases.FILES) if name.endswith(".nc"))
SAVED_DATA = ["gcode", "gcodeIndex", "units", "gcodeShift", "tolerance", "currentTool", "feedRate", "positioningMode",
              "zMoves", "gcodeFileUnits"]
SAVED_FILE = ["filename", "truncate", "absoluteFlag", "canvasScaleFactor", "prependString"]


class InlineThread:
    def __init__(self, target=None, **kwargs):
        self.target = target

    def start(self):
        self.target()


@pytest.fixture
def isolated(data, monkeypatch):
    monkeypatch.setattr("File.gcodeFile.threading.Thread", InlineThread)
    settings = copy.deepcopy(data.config.settings)
    saved = {name: copy.copy(getattr(data, name)) for name in SAVED_DATA}
    gcodeFile = data.gcodeFile
    savedFile = {name: getattr(gcodeFile, name) for name in SAVED_FILE}
    savedLines = list(gcodeFile.line3D)
    board = data.boardManager.currentBoard
    data.boardManager.currentBoard = copy.deepcopy(board)
    yield data
    data.config.settings = settings
    for name, value in saved.items():
        setattr(data, name, value)
    for name, value in savedFile.items():
        setattr(gcodeFile, name, value)
    gcodeFile.line3D[:] = savedLines
    data.boardManager.currentBoard = board
    for queue in (data.gcode_queue, data.ui_queue1, data.console_queue):
        drain(queue)


def matches(actual, expected, tolerance):
    if isinstance(expected, dict):
        return isinstance(actual, dict) and actual.keys() == expected.keys() and all(
            matches(actual[key], value, tolerance) for key, value in expected.items()
        )
    if isinstance(expected, list):
        return isinstance(actual, list) and len(actual) == len(expected) and all(
            matches(a, e, tolerance) for a, e in zip(actual, expected)
        )
    if isinstance(expected, float) and not isinstance(actual, bool):
        return actual == pytest.approx(expected, abs=tolerance, rel=0)
    return actual == expected and type(actual) == type(expected)


def check(relative, actual):
    path = os.path.join(FIXTURES, relative)
    normalised = json.loads(json.dumps({"tolerance": TOLERANCE, **actual}))
    if REGENERATE:
        with open(path, "w") as outfile:
            json.dump(normalised, outfile, separators=(",", ":"), ensure_ascii=False)
            outfile.write("\n")
    with open(path) as infile:
        expected = json.load(infile)
    assert matches(normalised, expected, expected["tolerance"])


@pytest.mark.parametrize("name", GCODE_FILES)
def test_each_gcode_file_reproduces_its_fixture(isolated, name):
    check(os.path.join("gcode", name[:-3] + ".json"), cases.gcode_case(isolated, name))


def test_the_streamed_line_checks_reproduce_their_fixture(isolated):
    loaded = [cases.load_case(isolated, name) for name in GCODE_FILES]
    check(os.path.join("gcode", "stream.json"), {"lines": cases.stream_case(isolated, cases.all_stream_lines(loaded))})


def test_board_cut_grids_reproduce_their_fixture(isolated):
    check(os.path.join("boards", "cut_grids.json"), {"cases": cases.cut_grid_cases(isolated)})


def test_board_trims_reproduce_their_fixture(isolated):
    check(os.path.join("boards", "trim.json"), {"cases": cases.trim_cases(isolated)})


def test_board_info_and_new_boards_reproduce_their_fixture(isolated):
    edits, beds = cases.info_cases(isolated)
    check(os.path.join("boards", "info.json"), {"edits": edits, "newBoards": beds})


def test_board_files_reproduce_their_fixture(isolated, tmp_path):
    check(os.path.join("boards", "files.json"), {"cases": cases.file_cases(isolated, str(tmp_path))})
