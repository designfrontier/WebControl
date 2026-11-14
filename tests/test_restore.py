import copy
import json
import os
import zipfile

import pytest

import Actions.actions as actions_module
from helpers import drain


@pytest.fixture
def restorable(data, tmp_path):
    """A backup zip whose webcontrol.json names a different port and drops one setting, plus a gcode file."""
    saved_settings = copy.deepcopy(data.config.settings)
    webcontrol = os.path.join(data.config.getHome(), ".WebControl")
    saved_file = open(os.path.join(webcontrol, "webcontrol.json")).read()
    backup = copy.deepcopy(data.config.settings)
    port = next(option for option in backup["Maslow Settings"] if option["key"] == "COMport")
    port["value"] = "/dev/ttyRESTORED"
    backup["Maslow Settings"] = [option for option in backup["Maslow Settings"] if option["key"] != "macro2_title"]
    archive = tmp_path / "wc_backup.zip"
    with zipfile.ZipFile(archive, "w") as handle:
        handle.writestr("webcontrol.json", json.dumps(backup))
        handle.writestr("gcode/restored.nc", "G0 X0\n")
    drain(data.gcode_queue)
    yield str(archive), webcontrol
    data.config.settings = saved_settings
    with open(os.path.join(webcontrol, "webcontrol.json"), "w") as handle:
        handle.write(saved_file)
    os.remove(os.path.join(webcontrol, "gcode", "restored.nc"))
    drain(data.gcode_queue)


def test_restore_extracts_into_the_webcontrol_directory(data, restorable, monkeypatch):
    archive, webcontrol = restorable
    targets = []
    real_extractall = zipfile.ZipFile.extractall
    monkeypatch.setattr(
        actions_module.zipfile.ZipFile,
        "extractall",
        lambda self, path=None, *args, **kwargs: (targets.append(path), real_extractall(self, path, *args, **kwargs))[1],
    )
    assert data.actions.restoreWebControl(archive) is True
    assert targets == [webcontrol]
    assert os.path.isfile(os.path.join(webcontrol, "gcode", "restored.nc"))


def test_restore_reloads_the_restored_settings(data, restorable):
    archive, _ = restorable
    assert data.actions.restoreWebControl(archive) is True
    assert data.config.getValue("Maslow Settings", "COMport") == "/dev/ttyRESTORED"
    assert data.config.getValue("Maslow Settings", "macro2_title") is not None
    assert "$$" in drain(data.gcode_queue)


def test_a_restore_without_settings_fails(data, tmp_path):
    archive = tmp_path / "empty.zip"
    with zipfile.ZipFile(archive, "w") as handle:
        handle.writestr("notes.txt", "")
    assert data.actions.restoreWebControl(str(archive)) is False
