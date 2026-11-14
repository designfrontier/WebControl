import base64
import gzip
import json
from types import SimpleNamespace

import pytest

from api.viewer import viewer_snapshot

MACHINE = {
    ("Maslow Settings", "bedWidth"): "2438.4",
    ("Maslow Settings", "bedHeight"): 1219.2,
    ("Maslow Settings", "motorOffsetY"): "463",
    ("Maslow Settings", "motorSpacingX"): "2978.4",
}
BOARD_INFO = {
    "width": 24,
    "height": 12,
    "thickness": 0.75,
    "centerX": 1,
    "centerY": -2,
    "boardID": "b1",
    "material": "Plywood",
    "fileName": "",
}


def gz(value):
    return gzip.compress(json.dumps(value).encode())


class FakeBoard:
    def __init__(self, cut_data, info=BOARD_INFO):
        self.cut_data = cut_data
        self.info = info

    def getBoardInfoJSON(self):
        return json.dumps(self.info)

    def getCompressedCutData(self):
        return self.cut_data


def fake_data(gcode=None, cut_data="", settings=None, board=None):
    values = {**MACHINE, **(settings or {})}
    return SimpleNamespace(
        config=SimpleNamespace(getValue=lambda section, key: values.get((section, key))),
        compressedGCode3D=gcode,
        boardManager=SimpleNamespace(getCurrentBoard=lambda: board or FakeBoard(cut_data)),
    )


def test_converts_the_machine_geometry_to_inches():
    assert viewer_snapshot(fake_data())["machine"] == {
        "bedWidth": 96.0,
        "bedHeight": 48.0,
        "motorOffsetY": 18.2283,
        "motorSpacingX": 117.2598,
    }


def test_unreadable_machine_settings_are_null():
    machine = viewer_snapshot(fake_data(settings={("Maslow Settings", "bedWidth"): "wide"}))["machine"]
    assert machine["bedWidth"] is None
    assert viewer_snapshot(fake_data(settings={("Maslow Settings", "bedHeight"): None}))["machine"]["bedHeight"] is None


def test_sends_gzip_payloads_as_base64():
    gcode = gz([{"type": "line", "points": [[0, 0, 0], [1, 1, 0]], "dashed": False}])
    cut = gz([True, False])
    snapshot = viewer_snapshot(fake_data(gcode=gcode, cut_data=cut))
    assert base64.b64decode(snapshot["gcode"]) == gcode
    assert base64.b64decode(snapshot["cutData"]) == cut
    assert snapshot["board"] == BOARD_INFO


@pytest.mark.parametrize("empty", [None, "", b""])
def test_nothing_loaded_is_null(empty):
    snapshot = viewer_snapshot(fake_data(gcode=empty, cut_data=empty))
    assert snapshot["gcode"] is None
    assert snapshot["cutData"] is None


def test_a_board_without_info_is_null():
    class Broken(FakeBoard):
        def getBoardInfoJSON(self):
            return "{nope"

    assert viewer_snapshot(fake_data(board=Broken("")))["board"] is None


def test_endpoint_serves_the_live_data(client, data):
    saved = data.compressedGCode3D
    data.compressedGCode3D = gz([])
    try:
        body = client.get("/api/viewer").get_json()
    finally:
        data.compressedGCode3D = saved
    assert set(body) == {"machine", "gcode", "board", "cutData"}
    assert body["machine"]["bedWidth"] == pytest.approx(
        float(data.config.getValue("Maslow Settings", "bedWidth")) / 25.4, abs=1e-4
    )
    assert json.loads(gzip.decompress(base64.b64decode(body["gcode"]))) == []
    assert body["board"]["boardID"] == data.boardManager.getCurrentBoard().boardID
