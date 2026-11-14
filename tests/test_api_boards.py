import os

import pytest

from api.boards import current_board


@pytest.fixture
def board(data):
    """The live current board, restored after the test."""
    current = data.boardManager.getCurrentBoard()
    fields = ["boardID", "material", "width", "height", "thickness", "centerX", "centerY", "boardFilename"]
    saved = {field: getattr(current, field) for field in fields}
    current.updateBoardInfo("Shelf", "Plywood", 24, 48, 0.75, 1.5, -2, "inches")
    current.boardFilename = "shelf.brd"
    yield current
    for field, value in saved.items():
        setattr(current, field, value)


@pytest.fixture
def boards_root(data):
    root = os.path.join(data.config.getHome(), ".WebControl", "boards")
    created = []

    def write(relative):
        path = os.path.join(root, relative)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as handle:
            handle.write("{}\n")
        created.append(path)

    yield root, write
    for path in created:
        os.remove(path)
    for name in os.listdir(root):
        full = os.path.join(root, name)
        if os.path.isdir(full) and not os.listdir(full):
            os.rmdir(full)


def test_reports_the_board_in_inches_when_the_machine_uses_inches(data, board):
    data.units, data.xval, data.yval = "INCHES", 3.0, -4.5
    assert current_board(data) == {
        "boardID": "Shelf",
        "material": "Plywood",
        "width": 48.0,
        "height": 24.0,
        "thickness": 0.75,
        "centerX": 1.5,
        "centerY": -2.0,
        "units": "inches",
        "fileName": "shelf.brd",
        "router": {"x": 3.0, "y": -4.5},
    }


def test_converts_the_board_to_millimetres_for_metric_machines(data, board):
    data.units = "MM"
    body = current_board(data)
    assert body["units"] == "mm"
    assert (body["width"], body["height"], body["thickness"]) == (1219.2, 609.6, 19.05)
    assert (body["centerX"], body["centerY"]) == (38.1, -50.8)


def test_current_endpoint(client, data, board):
    data.units = "INCHES"
    body = client.get("/api/boards/current").get_json()
    assert body["boardID"] == "Shelf"
    assert body["units"] == "inches"


def test_library_endpoint_lists_boards_and_prefers_the_current_file(client, data, board, boards_root):
    _, write = boards_root
    write("shelf.brd")
    write("shop/bench.brd")
    data.config.setValue("Computed Settings", "lastSelectedBoardDirectory", "shop")

    body = client.get("/api/boards/files").get_json()

    assert body["directories"] == [".", "shop"]
    assert body["files"] == [{"directory": ".", "name": "shelf.brd"}, {"directory": "shop", "name": "bench.brd"}]
    assert body["lastDirectory"] == "shop"
    assert body["lastFile"] == "shelf.brd"


def test_library_falls_back_to_the_open_board_setting(client, data, board):
    board.boardFilename = ""
    data.config.setValue("Maslow Settings", "openBoardFile", "old.brd")
    assert client.get("/api/boards/files").get_json()["lastFile"] == "old.brd"


def test_creates_board_directories(client, boards_root):
    root, _ = boards_root
    response = client.post("/api/boards/directories", json={"name": "shop"})
    assert response.status_code == 201
    assert response.get_json()["directories"] == [".", "shop"]
    assert os.path.isdir(os.path.join(root, "shop"))


def test_rejects_an_empty_board_directory_name(client):
    assert client.post("/api/boards/directories", json={"name": ""}).status_code == 400


def test_classic_edit_route_round_trips_through_the_current_board(client, data, board):
    """The new UI saves board edits through /editBoard with the fields and units the endpoint reports."""
    data.units = "MM"
    form = {**{k: str(v) for k, v in current_board(data).items() if k != "router"}, "boardID": "Bench"}
    assert client.post("/editBoard", data=form).status_code == 200
    body = current_board(data)
    assert body["boardID"] == "Bench"
    assert body["width"] == pytest.approx(1219.2, abs=0.3)


def test_opening_a_board_keeps_the_loaded_gcode_name(client, data, board, boards_root):
    _, write = boards_root
    write("shelf.brd")
    saved = (data.gcode, data.gcodeFile.filename)
    data.gcode = ["G0 X0", "G1 X1"]
    data.gcodeFile.filename = data.config.getHome() + "/.WebControl/gcode/leg.nc"
    try:
        client.post("/openBoard", data={"selectedBoard": "./shelf.brd"})
        loaded = client.get("/api/files/gcode/loaded").get_json()
    finally:
        data.gcode, data.gcodeFile.filename = saved
    assert loaded == {"directory": ".", "name": "leg.nc", "lineCount": 2}
