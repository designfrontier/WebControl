import json
import os

import pytest

import config.config as config_module
from config.config import Config


@pytest.fixture
def home(tmp_path, monkeypatch):
    monkeypatch.setenv("HOME", str(tmp_path))
    return tmp_path / ".WebControl"


def settings_file(home, suffix=""):
    return home / ("webcontrol.json" + suffix)


def read(path):
    with open(path) as handle:
        return json.load(handle)


def port_of(settings):
    return next(option["value"] for option in settings["Maslow Settings"] if option["key"] == "COMport")


def fresh_config(home):
    config = Config()
    config.data = None
    return config


def test_saving_keeps_the_previous_file_as_a_backup(home):
    config = fresh_config(home)
    config.setValue("Maslow Settings", "COMport", "/dev/ttyACM0", recursionBreaker=True)
    config.setValue("Maslow Settings", "COMport", "/dev/ttyACM1", recursionBreaker=True)
    assert port_of(read(settings_file(home))) == "/dev/ttyACM1"
    assert port_of(read(settings_file(home, ".bak"))) == "/dev/ttyACM0"
    assert not [name for name in os.listdir(home) if name.endswith(".tmp")]


def test_a_failed_write_leaves_the_settings_file_intact(home, monkeypatch):
    config = fresh_config(home)
    config.setValue("Maslow Settings", "COMport", "/dev/ttyACM0", recursionBreaker=True)
    before = settings_file(home).read_text()

    def crash(*args, **kwargs):
        raise OSError("disk full")

    monkeypatch.setattr(config_module.json, "dump", crash)
    with pytest.raises(OSError):
        config.setValue("Maslow Settings", "COMport", "/dev/ttyACM1", recursionBreaker=True)
    assert settings_file(home).read_text() == before
    assert not [name for name in os.listdir(home) if name.endswith(".tmp")]


def test_writes_are_flushed_to_disk_before_replacing(home, monkeypatch):
    config = fresh_config(home)
    calls = []
    monkeypatch.setattr(config_module.os, "fsync", lambda fd: calls.append("fsync"))
    real_replace = os.replace
    monkeypatch.setattr(config_module.os, "replace", lambda a, b: (calls.append("replace"), real_replace(a, b)))
    config.saveSettings()
    assert calls[0] == "fsync" and "replace" in calls


def test_a_corrupt_settings_file_falls_back_to_the_backup(home, capsys):
    config = fresh_config(home)
    config.setValue("Maslow Settings", "COMport", "/dev/ttyACM0", recursionBreaker=True)
    config.setValue("Maslow Settings", "COMport", "/dev/ttyACM1", recursionBreaker=True)
    settings_file(home).write_text('{"Maslow Settings": [')
    reloaded = fresh_config(home)
    assert port_of(reloaded.settings) == "/dev/ttyACM0"
    assert "webcontrol.json" in capsys.readouterr().out
    assert port_of(read(settings_file(home))) == "/dev/ttyACM0"


def test_corrupt_settings_and_backup_fall_back_to_the_defaults(home, capsys):
    fresh_config(home)
    settings_file(home).write_text("")
    settings_file(home, ".bak").write_text("not json")
    reloaded = fresh_config(home)
    assert port_of(reloaded.settings) == ""
    assert "defaults" in capsys.readouterr().out.lower()
    assert read(settings_file(home))["Maslow Settings"]


def test_a_missing_settings_file_with_a_backup_restores_the_backup(home):
    config = fresh_config(home)
    config.setValue("Maslow Settings", "COMport", "/dev/ttyACM0", recursionBreaker=True)
    config.setValue("Maslow Settings", "COMport", "/dev/ttyACM1", recursionBreaker=True)
    settings_file(home).unlink()
    reloaded = fresh_config(home)
    assert port_of(reloaded.settings) == "/dev/ttyACM0"
    assert reloaded.firstRun is False
