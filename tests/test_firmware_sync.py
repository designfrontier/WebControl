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


def set_value(config, key, value):
    option(config, key)["value"] = value


def sent_keys(data):
    return [line.split("=")[0] for line in queued(data)]


def test_a_value_the_firmware_prints_rounded_to_8_decimals_is_not_rewritten(data, config):
    set_value(config, "chainElasticity", 0.0000051685)

    config.receivedSetting("$45=0.00000517 (chain stretch factor, m/m/N)\r\n")

    assert sent_keys(data) == []


def test_a_real_difference_in_a_tiny_value_is_still_rewritten(data, config):
    set_value(config, "chainElasticity", 0.0000051685)

    config.receivedSetting("$45=0.00000600\r\n")

    assert sent_keys(data) == ["$45"]


@pytest.mark.parametrize("key,setting,stored,reply", [
    ("$10", "chainLength", "3360.6", "$10=3360\r\n"),
    ("$11", "chainExtendLength", "1651.5", "$11=1651\r\n"),
    ("$15", "maxFeedrate", "800.9", "$15=800\r\n"),
])
def test_fractional_unsigned_int_settings_match_what_the_firmware_stored(data, config, key, setting, stored, reply):
    set_value(config, setting, stored)

    config.receivedSetting(reply)

    assert sent_keys(data) == []


@pytest.mark.parametrize("reply", ["$43=nan (reserved1, deg)\r\n", "$43=nan\r\n", "$43=ovf\r\n"])
def test_a_nan_reply_is_rewritten_rather_than_read_as_the_digit_in_its_label(data, config, reply):
    set_value(config, "topBeamTilt", "1")

    config.receivedSetting(reply)

    assert queued(data) == ["$43=1"]


def test_a_spindle_relay_setting_the_controller_already_has_is_not_rewritten(data, config):
    set_value(config, "spindleAutomate", "Relay_High")

    config.receivedSetting("$17=2\r\n")

    assert sent_keys(data) == []
