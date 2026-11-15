"""Settings sync between WebControl's config and the controller's `$` keys, checked against the firmware's behaviour."""
import copy

import pytest

from helpers import drain
from test_serial_streaming import serial  # noqa: F401


@pytest.fixture
def config(data, monkeypatch):
    """The live config, with its settings and the firmware facts on data put back after each test."""
    saved = copy.deepcopy(data.config.settings)
    monkeypatch.setattr(data, "controllerFirmwareVersion", 51.27)
    monkeypatch.setattr(data.config, "saveSettings", lambda: None)
    drain(data.gcode_queue)
    yield data.config
    data.config.settings = saved
    drain(data.gcode_queue)


def option(config, key):
    return next(item for section in config.settings.values() for item in section if item["key"] == key)


def queued(data):
    return drain(data.gcode_queue)


def test_connecting_writes_dist_per_rot_even_when_the_controller_already_has_it(data, serial, config):
    config.receivedSetting("$13=63.50000000\r\n")
    assert not [line for line in queued(data) if line.startswith("$13=")]

    port = serial.run()

    assert [line for line in port.sent if line.startswith("$13=")] == ["$13=63.5000 "]
