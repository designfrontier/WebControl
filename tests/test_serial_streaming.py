"""Drives SerialPortThread against a fake port and a fake clock, so no hardware and no real waiting."""
import types
from collections import deque

import pytest

import Connection.serialPortThread as serial_port_thread
from Connection.serialPortThread import SerialPortThread
from helpers import decode_ui_message, drain

SAVED_DATA = [
    "gcode", "gcodeIndex", "positioningMode", "connectionStatus", "requestSerialClose", "currentZTarget",
    "currentTool", "pausedzval", "pausedUnits", "pausedPositioningMode", "gcodeShift",
]


class Clock:
    def __init__(self):
        self.now = 1000.0

    def time(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


class FakePort:
    """Answers every written line with `ok` when `ack` is set; `on_poll` runs once per pass of the read loop."""

    def __init__(self, ack, on_poll):
        self.ack = ack
        self.on_poll = on_poll
        self.incoming = deque(["ok\r\n"])
        self.written = []
        self.is_open = True
        self.parity = None

    @property
    def sent(self):
        return [line.rstrip("\n") for line in self.written]

    def open(self):
        self.is_open = True

    def close(self):
        self.is_open = False

    def _require_open(self):
        if not self.is_open:
            raise OSError("Attempting to use a port that is not open")

    @property
    def in_waiting(self):
        self.on_poll(self)
        self._require_open()
        return len(self.incoming)

    def readline(self):
        self._require_open()
        return self.incoming.popleft().encode()

    def write(self, payload):
        self._require_open()
        self.written.append(payload.decode())
        if self.ack:
            self.incoming.append("ok\r\n")


class SerialHarness:
    def __init__(self, data, monkeypatch):
        self.data = data
        self.clock = Clock()
        self.settings = {}
        self.port = None
        monkeypatch.setattr(serial_port_thread, "time", self.clock)
        monkeypatch.setattr(serial_port_thread, "serial", types.SimpleNamespace(
            VERSION="3.5", PARITY_ODD="O", PARITY_NONE="N", Serial=lambda *args, **kwargs: self.port,
        ))
        real_get_value = data.config.getValue
        monkeypatch.setattr(data.config, "getValue", lambda section, key: self.settings.get(key, real_get_value(section, key)))

    def thread(self, ack=True):
        """A thread wired to a fresh open port, for driving single methods."""
        self.port = FakePort(ack, lambda port: None)
        thread = SerialPortThread()
        thread.data = self.data
        thread.serialInstance = self.port
        return thread

    def run(self, ack=True, on_poll=lambda port: None, buffer=False):
        """Runs getmessage until it returns; with nothing read for 5 s of fake time it times out."""
        self.settings["bufferOn"] = "1" if buffer else "0"
        self.port = FakePort(ack, on_poll)
        thread = SerialPortThread()
        thread.data = self.data
        thread.getmessage()
        return self.port

    def ui_messages(self):
        return [decode_ui_message(raw) for raw in drain(self.data.ui_queue1)]


@pytest.fixture
def serial(data, monkeypatch):
    saved = {key: getattr(data, key) for key in SAVED_DATA}
    saved_request = data.serialPort.serialPortRequest
    data.gcode = []
    data.gcodeIndex = 0
    data.positioningMode = 0
    data.requestSerialClose = False
    data.serialPort.serialPortRequest = ""
    data.gcodeShift = [0.0, 0.0]
    yield SerialHarness(data, monkeypatch)
    drain(data.gcode_queue)
    for key, value in saved.items():
        setattr(data, key, value)
    data.serialPort.serialPortRequest = saved_request


def once_after(clock, seconds, action):
    """An on_poll hook that runs `action` the first time the fake clock passes `seconds` from now."""
    due = clock.now + seconds
    state = {"done": False}

    def hook(port):
        if not state["done"] and clock.now >= due:
            state["done"] = True
            action(port)

    return hook


def test_a_stale_gcode_index_does_not_kill_the_thread_and_stop_still_goes_out(data, serial):
    data.gcode = ["G0 X1 "]
    data.gcodeIndex = 5

    port = serial.run(on_poll=once_after(serial.clock, 1, lambda port: data.quick_queue.put("!")))

    assert "!" in port.sent
    assert data.connectionStatus == 0


def test_closing_for_a_firmware_upgrade_leaves_the_status_to_the_upgrade(data, serial):
    def request_close(port):
        data.serialPort.serialPortRequest = "requestToClose"

    port = serial.run(on_poll=once_after(serial.clock, 1, request_close))

    assert port.is_open is False
    assert data.serialPort.serialPortRequest == "Closed"
    assert data.connectionStatus == 1
    assert serial.clock.now < 1002


def test_a_reconnect_does_not_inherit_unacked_lines_from_the_old_connection(data, serial):
    serial.run(ack=False)
    drain(data.gcode_queue)

    port = serial.run()

    assert port.sent[:3] == ["B05  ", f"{'G20' if data.units == 'INCHES' else 'G21'}  ", "$$ "]


def test_a_requested_close_ends_the_thread_before_it_can_take_a_stop(data, serial):
    polls_after_close = []

    def poll(port):
        if not port.is_open:
            polls_after_close.append(serial.clock.now)
            data.quick_queue.put("!")
        elif serial.clock.now >= 1001:
            data.requestSerialClose = True

    port = serial.run(on_poll=poll)

    assert port.is_open is False
    assert data.requestSerialClose is False
    assert data.connectionStatus == 0
    assert polls_after_close == []


def test_losing_the_connection_mid_cut_pauses_the_run(data, serial):
    data.gcode = [f"G1 X{n} " for n in range(20)]
    data.uploadFlag = 1

    serial.run(on_poll=once_after(serial.clock, 1, lambda port: setattr(port, "ack", False)))

    assert data.uploadFlag == 2
    messages = serial.ui_messages()
    assert ("Action", "setAsResume", "") in messages
    assert any(command == "Alert" and message == "Connection Lost" for command, message, _ in messages)


def test_buffered_streaming_never_overfills_the_controller_buffer(data, serial):
    data.gcode = ["G1 X1 Y1 "] * 20
    data.gcodeShift = [1000.1234, 1000.1234]
    data.uploadFlag = 1
    skip_startup_commands = once_after(serial.clock, 0, lambda port: drain(data.gcode_queue))

    port = serial.run(ack=False, on_poll=skip_startup_commands, buffer=True)

    assert port.sent[0].startswith("G1 X1001.1234")
    assert sum(len(line) for line in port.written) <= SerialPortThread.bufferSize
