"""BoardManager.processGCode marks the 1-inch cells of the board that the loaded gcode cuts through."""
import pytest

from File.gcodeFile import Line


@pytest.fixture
def board(data):
    """A 48 x 24 inch board centred on 0,0 with no cuts, in an inch machine homed at 0,0."""
    current = data.boardManager.getCurrentBoard()
    fields = ["width", "height", "centerX", "centerY", "cutPoints"]
    saved = {field: getattr(current, field) for field in fields}
    saved_lines, saved_shift = list(data.gcodeFile.line3D), list(data.gcodeShift)
    current.updateBoardInfo("-", "-", 24, 48, 0.75, 0, 0, "inches")
    current.cutPoints = []
    data.units = "INCHES"
    data.gcodeShift = [0.0, 0.0]
    yield current
    for field, value in saved.items():
        setattr(current, field, value)
    data.gcodeFile.line3D[:] = saved_lines
    data.gcodeShift = saved_shift


def draw(data, kind, *points):
    line = Line()
    line.type = kind
    line.points = list(points)
    data.gcodeFile.line3D[:] = [line]


def cut_cells(board):
    width = 48
    return [(index % width, index // width) for index, cut in enumerate(board.cutPoints) if cut]


def test_a_plunge_above_the_board_is_not_marked_as_cut(data, board):
    draw(data, "circle", (0, 20, -1), (1, 0, -1))

    data.boardManager.processGCode()

    assert cut_cells(board) == []


def test_a_short_cut_is_checked_against_the_y_extent_not_home_x(data, board):
    data.gcodeShift = [30.0, 0.0]
    draw(data, "line", (-10, 0, -0.1), (-10, 0.1, 0.05))

    data.boardManager.processGCode()

    assert cut_cells(board) == [(44, 12)]
