import copy
import subprocess
from types import SimpleNamespace

import pytest

import Actions.actions as actions_module
import Actions.firmware as firmware
from helpers import decode_ui_message, drain

INJECTIONS = ["/dev/ttyACM0; rm -rf ~", "COM3 && calc", "$(reboot)", "/dev/ttyACM0`id`", "-Uflash:w:evil.hex", ""]


@pytest.fixture
def flash(data, tmp_path, monkeypatch):
    """Runs upgradeFirmware against a fake install root, a fake avrdude and a serial link that closes at once."""
    for folder in firmware.FIRMWARE_FOLDERS.values():
        (tmp_path / "firmware" / folder).mkdir(parents=True)
        (tmp_path / "firmware" / folder / f"{folder}-1.26.hex").write_text(":00000001FF\n")
    runs = []
    result = SimpleNamespace(returncode=0, stdout=b"avrdude done")

    def fake_run(*args, **kwargs):
        runs.append((args, kwargs))
        return result

    saved = (data.comport, data.connectionStatus, data.serialPort.serialPortRequest)
    monkeypatch.setattr(firmware, "installRoot", lambda data: str(tmp_path))
    monkeypatch.setattr(actions_module.subprocess, "run", fake_run)
    monkeypatch.setattr(actions_module.time, "sleep", lambda seconds: None)
    monkeypatch.setattr(firmware, "PORT_CLOSE_SECONDS", 0.05)
    monkeypatch.setattr(data.serialPort, "closeConnection", lambda: setattr(data.serialPort, "serialPortRequest", "Closed"))
    data.comport = "/dev/ttyACM0"
    data.connectionStatus = 1
    drain(data.ui_queue1)
    yield SimpleNamespace(root=tmp_path, runs=runs, result=result)
    data.comport, data.connectionStatus, data.serialPort.serialPortRequest = saved
    drain(data.ui_queue1)


def ui_messages(data):
    return [decode_ui_message(item) for item in drain(data.ui_queue1)]


def alerts(data, messages=None):
    return [payload for command, _, payload in (messages or ui_messages(data)) if command == "Alert"]


def spinner_cleared(messages):
    return ("Action", "closeModals", "Notification:") in messages


def test_avrdude_gets_an_argument_list_without_a_shell(data, flash):
    assert data.actions.upgradeFirmware(1) is True
    (args, kwargs), = flash.runs
    command = args[0]
    assert isinstance(command, list)
    assert not kwargs.get("shell")
    assert "-P/dev/ttyACM0" in command
    assert command[-1].startswith("-Uflash:w:") and command[-1].endswith("maslowcnc-1.26.hex:i")


@pytest.mark.parametrize("port", INJECTIONS)
def test_a_port_that_is_not_a_serial_device_is_never_flashed(data, flash, port):
    data.comport = port
    assert data.actions.upgradeFirmware(1) is False
    assert flash.runs == []
    assert any("serial port" in alert for alert in alerts(data))


@pytest.mark.parametrize(
    "port", ["/dev/ttyACM0", "/dev/ttyUSB1", "/dev/tty.usbmodem14101", "/dev/cu.usbmodem1", "COM3", "COM12",
             "/dev/serial/by-id/usb-Arduino__www.arduino.cc__0042_55639303035351A04171-if00"]
)
def test_serial_device_names_are_accepted(port):
    assert firmware.isSerialPortName(port)


@pytest.mark.parametrize("port", INJECTIONS + ["None", "/dev/../etc/passwd", "COM3x"])
def test_other_names_are_refused(port):
    assert not firmware.isSerialPortName(port)


def test_a_port_the_system_listed_is_accepted(data):
    assert firmware.isSerialPortName("/dev/weird-but-listed", ["/dev/weird-but-listed"])
    assert not firmware.isSerialPortName("None", ["None"])


@pytest.fixture
def settings(data):
    saved = copy.deepcopy(data.config.settings)
    yield data.config
    data.config.settings = saved


@pytest.mark.parametrize("port", INJECTIONS[:-1])
def test_settings_api_refuses_a_port_that_is_not_a_serial_device(client, settings, port):
    response = client.put("/api/settings/maslow", json={"values": {"COMport": port}})
    assert response.status_code == 400
    assert "COMport" in response.get_json()["errors"]
    assert settings.getValue("Maslow Settings", "COMport") != port


def test_settings_api_still_saves_a_serial_device(client, settings):
    assert client.put("/api/settings/maslow", json={"values": {"COMport": "/dev/ttyACM0"}}).status_code == 200
    assert settings.getValue("Maslow Settings", "COMport") == "/dev/ttyACM0"


def maslow_form(config, **overrides):
    form = {setting["key"]: str(setting["value"]) for setting in config.getJSONSettingSection("Maslow Settings")}
    return {**form, **overrides}


@pytest.mark.parametrize("port", INJECTIONS[:-1])
def test_classic_settings_form_refuses_a_port_that_is_not_a_serial_device(client, settings, port):
    response = client.post("/maslowSettings", data=maslow_form(settings, COMport=port))
    assert response.status_code == 400
    assert settings.getValue("Maslow Settings", "COMport") != port


def test_classic_settings_form_still_saves_a_serial_device(client, settings):
    assert client.post("/maslowSettings", data=maslow_form(settings, COMport="COM4")).status_code == 200
    assert settings.getValue("Maslow Settings", "COMport") == "COM4"


def test_flashing_waits_for_the_port_to_report_closed(data, flash, monkeypatch):
    monkeypatch.setattr(
        data.serialPort, "closeConnection", lambda: setattr(data.serialPort, "serialPortRequest", "requestToClose")
    )
    assert data.actions.upgradeFirmware(1) is False
    messages = ui_messages(data)
    assert flash.runs == []
    assert any("did not close" in alert for alert in alerts(data, messages))
    assert spinner_cleared(messages)


def test_a_disconnected_port_is_flashed_without_waiting(data, flash, monkeypatch):
    data.connectionStatus = 0
    monkeypatch.setattr(data.serialPort, "closeConnection", lambda: None)
    assert data.actions.upgradeFirmware(1) is True
    assert len(flash.runs) == 1


def test_success_clears_the_spinner_and_lets_the_port_reconnect(data, flash):
    assert data.actions.upgradeFirmware(1) is True
    assert spinner_cleared(ui_messages(data))
    assert data.connectionStatus == 0
    assert data.serialPort.serialPortRequest == ""


def test_an_avrdude_failure_is_alerted_and_clears_the_spinner(data, flash):
    flash.result.returncode = 1
    flash.result.stdout = b"avrdude: ser_open(): can't open device"
    assert data.actions.upgradeFirmware(1) is False
    messages = ui_messages(data)
    assert any("can't open device" in alert for alert in alerts(data, messages))
    assert spinner_cleared(messages)
    assert data.serialPort.serialPortRequest == ""


def test_a_missing_hex_file_is_alerted_and_clears_the_spinner(data, flash):
    (flash.root / "firmware" / "holey" / "holey-1.26.hex").unlink()
    assert data.actions.upgradeFirmware(2) is False
    messages = ui_messages(data)
    assert flash.runs == []
    assert any("No firmware file" in alert for alert in alerts(data, messages))
    assert spinner_cleared(messages)


def test_a_missing_avrdude_is_alerted_and_clears_the_spinner(data, flash, monkeypatch):
    def missing(*args, **kwargs):
        raise FileNotFoundError("tools/avrdude")

    monkeypatch.setattr(actions_module.subprocess, "run", missing)
    assert data.actions.upgradeFirmware(1) is False
    messages = ui_messages(data)
    assert any("tools/avrdude" in alert for alert in alerts(data, messages))
    assert spinner_cleared(messages)
