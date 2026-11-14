import io
import json
import os

import pytest

import api.system
from File.safePaths import UnsafePathError, resolveInside
from helpers import NAMESPACE, drain, messages

ESCAPES = [
    "../outside.nc",
    "jobs/../../outside.nc",
    "/etc/passwd",
    "..\\outside.nc",
    "%2e%2e/outside.nc",
    "..%2foutside.nc",
    "%252e%252e%252foutside.nc",
    "C:\\Windows\\win.ini",
    "jobs/\x00.nc",
]


@pytest.fixture
def outside(tmp_path):
    """A root directory, and a directory next to it that a symlink inside the root points at."""
    root = tmp_path / "root"
    elsewhere = tmp_path / "elsewhere"
    root.mkdir()
    elsewhere.mkdir()
    (elsewhere / "secret.nc").write_text("G0 X1\n")
    (root / "link").symlink_to(elsewhere)
    return root, elsewhere


@pytest.mark.parametrize("relative", ["part.nc", "./part.nc", "jobs/part.nc", ".", "", "a..b.nc"])
def test_resolves_paths_inside_the_root(tmp_path, relative):
    resolved = resolveInside(str(tmp_path), relative)
    assert os.path.realpath(resolved).startswith(os.path.realpath(str(tmp_path)))


@pytest.mark.parametrize("relative", ESCAPES)
def test_rejects_paths_that_leave_the_root(tmp_path, relative):
    with pytest.raises(UnsafePathError):
        resolveInside(str(tmp_path), relative)


def test_rejects_symlinks_that_point_outside_the_root(outside):
    root, _ = outside
    with pytest.raises(UnsafePathError):
        resolveInside(str(root), "link/secret.nc")
    with pytest.raises(UnsafePathError):
        resolveInside(str(root), "link")


def test_joins_directory_and_file_parts(tmp_path):
    assert resolveInside(str(tmp_path), "./", "part.nc") == os.path.join(str(tmp_path), "part.nc")
    with pytest.raises(UnsafePathError):
        resolveInside(str(tmp_path), "..", "part.nc")


@pytest.fixture
def library(data):
    """The gcode and boards roots, with everything a test writes removed afterwards."""
    webcontrol = os.path.join(data.config.getHome(), ".WebControl")
    before = {path for path, _, _ in os.walk(data.config.getHome())}
    saved = (data.gcode, data.gcodeFile.filename)
    yield webcontrol
    data.gcode, data.gcodeFile.filename = saved
    for path in sorted({path for path, _, _ in os.walk(data.config.getHome())} - before, reverse=True):
        for name in os.listdir(path):
            full = os.path.join(path, name)
            os.remove(full) if os.path.isfile(full) or os.path.islink(full) else None
        os.rmdir(path)


def home_files(data):
    return {os.path.relpath(os.path.join(path, name), data.config.getHome()) for path, _, names in os.walk(data.config.getHome()) for name in names}


@pytest.mark.parametrize("directory", ["..", "../..", "/tmp", "%2e%2e"])
def test_save_gcode_rejects_a_directory_outside_the_library(client, data, library, directory):
    data.gcode = ["G0 X0"]
    before = home_files(data)
    response = client.post("/saveGCode", data={"fileName": "evil.nc", "selectedDirectory": directory})
    assert response.status_code == 400
    assert home_files(data) == before


@pytest.mark.parametrize("name", ["../evil.nc", "../../evil.nc", "/tmp/evil.nc"])
def test_save_gcode_rejects_a_file_name_outside_the_library(client, data, library, name):
    data.gcode = ["G0 X0"]
    response = client.post("/saveGCode", data={"fileName": name, "selectedDirectory": "."})
    assert response.status_code == 400


def test_save_gcode_still_saves_inside_the_library(client, data, library):
    data.gcode = ["G0 X0"]
    response = client.post("/saveGCode", data={"fileName": "ok.nc", "selectedDirectory": "./"})
    assert response.status_code == 200
    assert os.path.isfile(os.path.join(library, "gcode", "ok.nc"))


def test_upload_gcode_rejects_a_directory_outside_the_library(client, data, library):
    before = home_files(data)
    response = client.post(
        "/uploadGCode",
        data={"selectedDirectory": "..", "file[]": (io.BytesIO(b"G0 X0\n"), "evil.nc")},
        content_type="multipart/form-data",
    )
    assert response.status_code == 400
    assert home_files(data) == before


def test_upload_gcode_still_uploads_inside_the_library(client, data, library):
    response = client.post(
        "/uploadGCode",
        data={"selectedDirectory": ".", "file[]": (io.BytesIO(b"G0 X0\n"), "up.nc")},
        content_type="multipart/form-data",
    )
    assert response.status_code == 200
    assert os.path.isfile(os.path.join(library, "gcode", "up.nc"))


@pytest.mark.parametrize("selected", ["../webcontrol.json", "./../webcontrol.json", "/etc/hosts", "%2e%2e/webcontrol.json"])
def test_open_gcode_rejects_files_outside_the_library(client, data, library, selected):
    data.gcodeFile.filename = "unchanged"
    assert client.post("/openGCode", data={"selectedGCode": selected}).status_code == 400
    assert data.gcodeFile.filename == "unchanged"


def test_open_gcode_rejects_a_symlink_out_of_the_library(client, data, library, tmp_path):
    (tmp_path / "secret.nc").write_text("G0 X1\n")
    os.symlink(str(tmp_path), os.path.join(library, "gcode", "escape"))
    assert client.post("/openGCode", data={"selectedGCode": "escape/secret.nc"}).status_code == 400


@pytest.mark.parametrize("selected", ["../webcontrol.json", "/etc/hosts"])
def test_open_board_rejects_files_outside_the_library(client, data, library, selected):
    assert client.post("/openBoard", data={"selectedBoard": selected}).status_code == 400


@pytest.mark.parametrize("directory, name", [("..", "evil.brd"), (".", "../evil.brd"), ("/tmp", "evil.brd")])
def test_save_board_rejects_paths_outside_the_library(client, data, library, directory, name):
    before = home_files(data)
    response = client.post("/saveBoard", data={"fileName": name, "selectedDirectory": directory})
    assert response.status_code == 400
    assert home_files(data) == before


def test_create_directory_action_refuses_to_leave_the_library(data, library):
    data.actions.processAction({"data": {"command": "createDirectory", "arg": "../escaped", "arg1": ""}})
    assert not os.path.exists(os.path.join(library, "escaped"))
    drain(data.ui_queue1)


def test_create_directory_action_still_creates_library_directories(data, library):
    data.actions.processAction({"data": {"command": "createDirectory", "arg": "made", "arg1": ""}})
    assert os.path.isdir(os.path.join(library, "gcode", "made"))
    drain(data.ui_queue1)


def request_page(socket_client, page):
    socket_client.get_received(NAMESPACE)
    socket_client.emit("requestPage", {"data": {"page": page, "isMobile": False, "args": ""}}, namespace=NAMESPACE)
    return messages(socket_client, "activateModal")


@pytest.mark.parametrize("page", ["../README.md", "assets/../../README.md", "Actions/../../main.py", "%2e%2e/README.md"])
def test_request_page_does_not_render_files_outside_docs(socket_client, data, page):
    assert request_page(socket_client, page) == []
    drain(data.ui_queue1)


def test_request_page_still_renders_help_pages(socket_client, data):
    modals = request_page(socket_client, "Boards/index.md")
    assert [modal["data"]["title"] for modal in modals] == ["Help"]


@pytest.fixture
def fake_docs(tmp_path, monkeypatch, outside):
    root, elsewhere = outside
    (elsewhere / "secret.md").write_text("# secret\n")
    (root / "page.md").write_text("# page\n")
    (root / "assets").mkdir()
    (root / "assets" / "link").symlink_to(elsewhere)
    monkeypatch.setattr(api.system, "docs_root", lambda data: str(root))
    return root


def test_help_refuses_a_symlink_out_of_docs(client, fake_docs):
    assert client.get("/api/system/help?path=link/secret.md").status_code == 404
    assert client.get("/api/system/help?path=page.md").status_code == 200


@pytest.mark.parametrize("path", ["link/secret.nc", "link/secret.md", "..%2fpage.md"])
def test_help_assets_refuse_paths_out_of_the_assets_folder(client, fake_docs, path):
    assert client.get("/api/system/help/assets/" + path).status_code == 404


def test_gcode_directory_api_refuses_a_symlink_out_of_the_library(client, data, library, tmp_path):
    os.symlink(str(tmp_path), os.path.join(library, "gcode", "out"))
    response = client.post("/api/files/gcode/directories", data=json.dumps({"name": "out"}), content_type="application/json")
    assert response.status_code == 400
