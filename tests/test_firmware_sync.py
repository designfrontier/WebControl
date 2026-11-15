"""Settings sync between WebControl's config and the controller's `$` keys, checked against the firmware's behaviour."""
import copy
import types

import pytest

from Background.messageProcessor import MessageProcessor
from Connection import firmwareFacts
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


ADOPTED = {"maxZRPM": 18, "axisDetachTime": 9, "propWeightV": 28, "propWeightVZ": 36}


@pytest.mark.parametrize("key,firmware_key", ADOPTED.items())
def test_the_never_synced_keys_are_advanced_settings_that_start_unknown(config, key, firmware_key):
    setting = next(item for item in config.defaults["Advanced Settings"] if item["key"] == key)

    assert setting["firmwareKey"] == firmware_key
    assert setting["value"] == ""
    assert setting["title"] and setting["desc"]


@pytest.mark.parametrize("key,firmware_key", ADOPTED.items())
def test_an_unknown_key_adopts_the_controllers_value_without_writing_it(data, config, key, firmware_key):
    set_value(config, key, "")

    config.receivedSetting(f"${firmware_key}=126.00000000\r\n")

    assert option(config, key)["value"] == "126"
    assert queued(data) == []


def test_an_unknown_key_does_not_adopt_a_nan(data, config):
    set_value(config, "maxZRPM", "")

    config.receivedSetting("$18=nan\r\n")

    assert option(config, "maxZRPM")["value"] == ""
    assert queued(data) == []


def test_an_adopted_key_is_then_kept_in_sync(data, config):
    set_value(config, "maxZRPM", "12.6")

    config.receivedSetting("$18=126.00000000\r\n")

    assert queued(data) == ["$18=12.6"]


def test_saving_an_unknown_key_unchanged_sends_nothing_and_setting_it_sends_it(data, config):
    set_value(config, "maxZRPM", "")

    config.setValue("Advanced Settings", "maxZRPM", "", recursionBreaker=True)
    assert queued(data) == []

    config.setValue("Advanced Settings", "maxZRPM", "12.6", recursionBreaker=True)
    assert queued(data) == ["$18=12.6"]


def test_an_eeprom_read_fail_restores_every_setting_and_tells_the_user(data, config):
    set_value(config, "maxZRPM", "")
    drain(data.ui_queue1)
    processor = MessageProcessor()
    processor.data = data

    processor.processMessage("error: EEPROM read fail. Using default settings.\r\n")

    sent = queued(data)
    keys = [line.split("=")[0] for line in sent]
    assert {"$13", "$18", "$9", "$28", "$36", "$0", "$2", "$45", "$46"} <= set(keys)
    assert not {"$85", "$86", "$144", "$87"} & set(keys)
    assert "$18=12.6" in sent
    assert keys[-1] == "$$"
    assert option(config, "maxZRPM")["value"] == "12.6"
    assert len([message for message in drain(data.ui_queue1) if "lost its settings" in message]) == 1
    drain(data.ui_controller_queue)


@pytest.fixture
def processor(data, config):
    processor = MessageProcessor()
    processor.data = data
    drain(data.ui_controller_queue)
    yield processor
    drain(data.ui_controller_queue)
    drain(data.ui_queue1)


def test_the_firmware_version_reaches_the_controller_messages_pane_and_is_parsed(data, processor):
    processor.processMessage("Firmware Version 51.29\r\n")

    assert data.controllerFirmwareVersion == 51.29
    assert drain(data.ui_controller_queue) == ["Firmware Version 51.29\r\n"]


@pytest.mark.parametrize("line", ["Firmware Version ?\r\n", "Firmware update needed\r\n"])
def test_a_firmware_line_without_a_version_leaves_the_version_alone(data, processor, line):
    processor.processMessage(line)

    assert data.controllerFirmwareVersion == 51.27


BOOT = ["PCB v1.6 TLE9201  Detected\r\n", "FAKE_SERVO off\r\n", "Grbl v1.00\r\n", "ready\r\n", "ok\r\n"]


@pytest.fixture
def clock(monkeypatch, data):
    import Background.messageProcessor as message_processor

    fake = types.SimpleNamespace(now=5000.0)
    monkeypatch.setattr(message_processor, "time", types.SimpleNamespace(time=lambda: fake.now))
    monkeypatch.setattr(data, "lastBootBannerTime", 0.0)
    monkeypatch.setattr(data, "controllerBoard", None)
    return fake


def test_a_boot_asks_for_the_version_and_rewrites_dist_per_rot_once(data, processor, clock):
    [processor.processMessage(line) for line in BOOT]

    assert queued(data) == ["B05 ", "$13=63.5000"]
    assert data.controllerBoard == "PCB v1.6 TLE9201  Detected"
    assert [line for line in drain(data.ui_controller_queue) if "PCB" in line] == [BOOT[0]]


def test_every_later_boot_asks_again(data, processor, clock):
    [processor.processMessage(line) for line in BOOT]
    queued(data)
    clock.now += 30

    [processor.processMessage(line) for line in BOOT[2:]]

    assert queued(data) == ["B05 ", "$13=63.5000"]


@pytest.mark.parametrize("line", ["set prescaler to Grbl v1.00\r\n", "\x00ready\r\n", "junkPCB v1.5b TB6643 Detected\r\n"])
def test_a_banner_glued_to_other_text_is_still_a_boot(data, processor, clock, line):
    processor.processMessage(line)

    assert queued(data)[0] == "B05 "


@pytest.mark.parametrize("version,rewrite_notice,build", [
    (0, False, None),
    (51.27, False, "51.27"),
    (51.29, False, "51.29x"),
    (51.29, True, "51.292"),
    (51.293, True, "51.293"),
    (1.26, False, "1.26"),
])
def test_the_firmware_build_tells_51_292_by_its_eeprom_rewrite_notice(version, rewrite_notice, build):
    facts = types.SimpleNamespace(controllerFirmwareVersion=version, firmwareRewriteNoticeSeen=rewrite_notice)

    assert firmwareFacts.firmwareBuild(facts) == build


def test_the_rewrite_notice_marks_51_292_until_the_next_boot(data, processor, clock, monkeypatch):
    monkeypatch.setattr(data, "firmwareRewriteNoticeSeen", False)
    [processor.processMessage(line) for line in BOOT + ["Firmware Version 51.29\r\n"]]
    assert firmwareFacts.firmwareBuild(data) == "51.29x"

    processor.processMessage("Change in setting, rewriting EEPROM\r\n")
    assert firmwareFacts.firmwareBuild(data) == "51.292"

    clock.now += 30
    processor.processMessage(BOOT[0])
    assert firmwareFacts.firmwareBuild(data) == "51.29x"
    queued(data)


def test_already_is_not_a_boot(data, processor, clock):
    processor.processMessage("Message: the sled is already there\r\n")

    assert queued(data) == []
