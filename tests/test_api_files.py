import os
from types import SimpleNamespace

import pytest

from api.files import directory_setting, list_library, loaded_gcode
from helpers import decode_ui_message, drain


@pytest.fixture
def gcode_root(data):
    root = os.path.join(data.config.getHome(), ".WebControl", "gcode")
    created = []

    def write(relative, text="G0 X0\n"):
        path = os.path.join(root, relative)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as handle:
            handle.write(text)
        created.append(path)
        return path

    yield root, write
    for path in created:
        os.remove(path)
    for name in os.listdir(root):
        full = os.path.join(root, name)
        if os.path.isdir(full) and not os.listdir(full):
            os.rmdir(full)


@pytest.fixture
def restore_gcode(data):
    saved = (data.gcode, data.gcodeFile.filename, data.sentCustomGCode)
    yield
    data.gcode, data.gcodeFile.filename, data.sentCustomGCode = saved


def test_lists_root_and_subdirectory_files_sorted_without_hidden_entries(tmp_path):
    (tmp_path / "b.nc").write_text("")
    (tmp_path / "A.nc").write_text("")
    (tmp_path / ".DS_Store").write_text("")
    (tmp_path / "parts").mkdir()
    (tmp_path / "parts" / "leg.nc").write_text("")
    (tmp_path / ".git").mkdir()
    assert list_library(str(tmp_path)) == {
        "directories": [".", "parts"],
        "files": [
            {"directory": ".", "name": "A.nc"},
            {"directory": ".", "name": "b.nc"},
            {"directory": "parts", "name": "leg.nc"},
        ],
    }


def test_a_missing_library_lists_only_the_root(tmp_path):
    assert list_library(str(tmp_path / "missing")) == {"directories": ["."], "files": []}


@pytest.mark.parametrize("stored, expected", [(None, "."), ("", "."), ("./", "."), (".", "."), ("parts/", "parts")])
def test_normalises_the_stored_directory(stored, expected):
    assert directory_setting(stored) == expected


def fake_data(gcode, filename, home="/home/me"):
    config = SimpleNamespace(getHome=lambda: home)
    return SimpleNamespace(gcode=gcode, gcodeFile=SimpleNamespace(filename=filename), config=config)


@pytest.mark.parametrize(
    "filename, directory, name",
    [
        ("/home/me/.WebControl/gcode/./part.nc", ".", "part.nc"),
        ("/home/me/.WebControl/gcode/jobs/part.nc", "jobs", "part.nc"),
        ("/home/me/.WebControl/gcode/a/b/part.nc", None, "part.nc"),
        ("/elsewhere/part.nc", None, "part.nc"),
    ],
)
def test_describes_where_the_loaded_gcode_came_from(filename, directory, name):
    assert loaded_gcode(fake_data(["G0 X0 ", "G1 X1 "], filename)) == {
        "directory": directory,
        "name": name,
        "lineCount": 2,
    }


@pytest.mark.parametrize("gcode, filename", [([], "/home/me/.WebControl/gcode/part.nc"), ("", ""), (["G0 "], "")])
def test_nothing_is_loaded_without_lines_or_a_file(gcode, filename):
    assert loaded_gcode(fake_data(gcode, filename))["name"] is None


def test_library_endpoint_lists_files_and_last_selection(client, data, gcode_root):
    _, write = gcode_root
    write("part.nc")
    write("jobs/leg.nc")
    data.config.setValue("Computed Settings", "lastSelectedDirectory", "jobs/")
    data.config.setValue("Maslow Settings", "openFile", "leg.nc")

    body = client.get("/api/files/gcode").get_json()

    assert body["directories"] == [".", "jobs"]
    assert {"directory": "jobs", "name": "leg.nc"} in body["files"]
    assert {"directory": ".", "name": "part.nc"} in body["files"]
    assert body["lastDirectory"] == "jobs"
    assert body["lastFile"] == "leg.nc"
    assert ".nc" in body["validExtensions"]


def test_creates_a_sanitised_directory_and_tells_the_classic_ui(client, data, gcode_root):
    root, _ = gcode_root
    drain(data.ui_queue1)

    response = client.post("/api/files/gcode/directories", json={"name": " My Parts/../x "})

    assert response.status_code == 201
    body = response.get_json()
    assert body["directory"] == "My_Parts_.._x"
    assert "My_Parts_.._x" in body["directories"]
    assert os.path.isdir(os.path.join(root, "My_Parts_.._x"))
    assert [decode_ui_message(m) for m in drain(data.ui_queue1)] == [
        ("Action", "updateDirectories", {"directory": "My_Parts_.._x"})
    ]


def test_creating_an_existing_directory_succeeds(client, gcode_root):
    root, write = gcode_root
    write("jobs/a.nc")
    assert client.post("/api/files/gcode/directories", json={"name": "jobs"}).status_code == 201


@pytest.mark.parametrize("body", [{"name": "  "}, {"name": "../"}, {}, None])
def test_rejects_directory_names_that_sanitise_to_nothing(client, data, body):
    drain(data.ui_queue1)
    url = "/api/files/gcode/directories"
    response = client.post(url, json=body) if body is not None else client.post(url, data="nope")
    assert response.status_code == 400
    assert "directory name" in response.get_json()["error"]
    assert drain(data.ui_queue1) == []


def test_loaded_text_and_custom_gcode_endpoints(client, data, restore_gcode):
    home = data.config.getHome()
    data.gcode = ["G0 X0 ", "G1 X1 ", ""]
    data.gcodeFile.filename = home + "/.WebControl/gcode/jobs/leg.nc"
    data.sentCustomGCode = "G0 X5\nG0 Y5"

    assert client.get("/api/files/gcode/loaded").get_json() == {"directory": "jobs", "name": "leg.nc", "lineCount": 3}
    assert client.get("/api/files/gcode/text").get_json() == {"text": "G0 X0 \nG1 X1"}
    assert client.get("/api/files/gcode/custom").get_json() == {"text": "G0 X5\nG0 Y5"}


def test_text_is_empty_when_the_gcode_was_never_loaded(client, data, restore_gcode):
    data.gcode = ""
    data.sentCustomGCode = None
    assert client.get("/api/files/gcode/text").get_json() == {"text": ""}
    assert client.get("/api/files/gcode/custom").get_json() == {"text": ""}


def test_classic_open_route_loads_a_listed_file(client, data, gcode_root, restore_gcode):
    """The new UI opens files through /openGCode with the directory/name pair the listing returns."""
    _, write = gcode_root
    write("jobs/leg.nc", "G21\nG0 X1 Y1\nG1 Z-1\n")
    assert client.post("/openGCode", data={"selectedGCode": "jobs/leg.nc"}).status_code == 200
    loaded = client.get("/api/files/gcode/loaded").get_json()
    assert (loaded["directory"], loaded["name"]) == ("jobs", "leg.nc")
    assert loaded["lineCount"] >= 3
