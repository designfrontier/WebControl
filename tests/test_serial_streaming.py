"""Drives SerialPortThread against a fake port and a fake clock, so no hardware and no real waiting."""
import types
from collections import deque

import pytest

import Connection.serialPortThread as serial_port_thread
from Connection.serialPortThread import SerialPortThread
from helpers import decode_ui_message, drain

SAVED_DATA = [
    "gcode", "gcodeIndex", "positioningMode", "connectionStatus", "requestSerialClose", "currentZTarget",
    "currentTool", "pausedzval", "pausedUnits", "pausedPositioningMode", "gcodeShift", "uploadFlag",
    "jobInProgress",
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


def send_lines(data, serial, lines):
    data.gcode = lines
    thread = serial.thread()
    [thread.sendNextLine() for _ in lines]
    return serial.port.sent


def test_a_relative_move_is_not_home_shifted_when_g91_shares_its_line(data, serial):
    data.gcodeShift = [100.0, 50.0]

    sent = send_lines(data, serial, ["G91 G0 X10 "])

    assert "X10 " in sent[0]
    assert data.positioningMode == 1


def test_an_absolute_move_is_home_shifted_when_g90_shares_its_line(data, serial):
    data.gcodeShift = [100.0, 50.0]
    data.positioningMode = 1

    sent = send_lines(data, serial, ["G90 G0 X10 "])

    assert "X110" in sent[0]
    assert data.positioningMode == 0


def test_the_z_target_stays_absolute_through_relative_moves(data, serial):
    send_lines(data, serial, ["G0 Z5 ", "G91 ", "G0 Z-2 ", "G1 X3 Z-1.5 ", "G90 G0 Z4 ", "G91 G0 Z1 "])

    assert data.currentZTarget == 5


def test_the_z_target_follows_absolute_moves(data, serial):
    data.currentZTarget = 9

    send_lines(data, serial, ["G0 Z5 ", "G1 X1 Y1 ", "G0 Z-0.5 "])

    assert data.currentZTarget == -0.5


def test_a_failure_after_writing_a_line_does_not_send_it_again(data, serial, monkeypatch):
    def broken_update(*args, **kwargs):
        raise RuntimeError("ui update failed")

    monkeypatch.setattr(data.actions, "sendGCodePositionUpdate", broken_update)
    data.gcode = ["G1 X1 ", "G1 X2 ", "G1 X3 "]
    data.uploadFlag = 1
    skip_startup_commands = once_after(serial.clock, 0, lambda port: drain(data.gcode_queue))

    port = serial.run(on_poll=skip_startup_commands)

    assert port.sent == ["G1 X1  ", "G1 X2  ", "G1 X3  "]
    assert data.uploadFlag == 0


def stream(data, serial, lines, buffer=False):
    data.gcode = lines
    data.uploadFlag = 1
    data.jobInProgress = True
    skip_startup_commands = once_after(serial.clock, 0, lambda port: drain(data.gcode_queue))
    return serial.run(on_poll=skip_startup_commands, buffer=buffer)


def alerts(serial):
    return [(title, text) for command, title, text in serial.ui_messages() if command == "Alert"]


# 129 characters; the home shift adds a space and the newline makes 131 bytes on the wire.
LONG_LINE = "G1 X1 Y1 " + "F100 " * 24


@pytest.mark.parametrize("buffer", [False, True])
def test_a_line_too_long_for_the_controller_pauses_the_run_instead_of_going_out(data, serial, buffer):
    port = stream(data, serial, ["G1 X0 ", LONG_LINE, "G1 X2 "], buffer=buffer)

    assert port.sent == ["G1 X0  "]
    assert (data.uploadFlag, data.jobInProgress, data.gcodeIndex) == (2, True, 1)
    assert ("Line Too Long", "Line 2 is 131 characters; the controller accepts at most 126. Resume stops here "
            "again: stop the job, fix the file (e.g. enable Truncate Floating Point Numbers) and load it again.") \
        in alerts(serial)


@pytest.mark.parametrize("buffer", [False, True])
def test_a_line_that_just_fits_the_controller_buffer_goes_out(data, serial, buffer):
    line = "G1 X1" + " " * 119  # the home shift appends a space, so 126 bytes are written

    port = stream(data, serial, [line, "G1 X2 "], buffer=buffer)

    assert [len(written) for written in port.written] == [126, 8]
    assert data.uploadFlag == 0


def test_the_home_shift_can_push_a_line_over_the_limit(data, serial):
    data.gcodeShift = [1000.1234, 1000.1234]
    line = "G1 X1 Y1" + " " * 112

    port = stream(data, serial, [line], buffer=True)

    assert port.written == []
    assert data.uploadFlag == 2


def test_a_queued_command_too_long_for_the_controller_is_refused(data, serial):
    def queue_commands(port):
        drain(data.gcode_queue)
        data.gcode_queue.put(LONG_LINE)
        data.gcode_queue.put("G0 X1")

    port = serial.run(on_poll=once_after(serial.clock, 0, queue_commands))

    assert port.sent == ["G0 X1 "]
    assert ("Command Too Long", "That command is 131 characters; the controller accepts at most 126.") in alerts(serial)


def test_closing_the_port_mid_cut_pauses_the_run(data, serial):
    data.gcode = [f"G1 X{n} " for n in range(500)]
    data.uploadFlag = 1

    serial.run(on_poll=once_after(serial.clock, 1, lambda port: setattr(data, "requestSerialClose", True)))

    assert data.uploadFlag == 2
    assert ("Action", "setAsResume", "") in serial.ui_messages()


def test_a_new_connection_puts_the_controller_and_the_host_back_in_absolute_mode(data, serial):
    data.positioningMode = 1

    port = serial.run()

    assert "G90  " in port.sent
    assert data.positioningMode == 0


def test_a_stop_while_a_line_is_being_written_leaves_the_index_at_the_start(data, serial):
    data.gcode = ["G1 X1 ", "G1 X2 ", "G1 X3 "]
    data.gcodeIndex = 1
    data.uploadFlag = 1
    thread = serial.thread()
    write = thread.serialInstance.write

    def write_then_stop(payload):
        write(payload)
        data.actions.stopRun()

    thread.serialInstance.write = write_then_stop
    thread.sendNextLine()

    assert (data.gcodeIndex, data.uploadFlag) == (0, 0)


@pytest.mark.parametrize("buffer", [False, True])
def test_a_line_with_an_unclosed_comment_pauses_the_run(data, serial, buffer):
    port = stream(data, serial, ["G1 X0 ", "G1 X1 (rough pass", "G1 X2 "], buffer=buffer)

    assert port.sent == ["G1 X0  "]
    assert (data.uploadFlag, data.gcodeIndex) == (2, 1)
    assert ("Line Unclosed Comment", "Line 2 has an unclosed comment, which hangs the controller. Resume stops "
            "here again: stop the job, fix the file and load it again.") in alerts(serial)


def test_resuming_after_a_refused_line_stops_at_that_line_again(data, serial):
    def resume_once_paused(port):
        if data.uploadFlag == 2 and not alerts_seen:
            alerts_seen.append(True)
            data.actions.resumeRun()
            drain(data.gcode_queue)

    alerts_seen = []
    data.gcode = ["G1 X0 ", "G1 X1 (rough pass", "G1 X2 "]
    data.uploadFlag = 1
    data.jobInProgress = True
    port = serial.run(on_poll=every_poll(
        once_after(serial.clock, 0, lambda port: drain(data.gcode_queue)), resume_once_paused,
    ))

    assert "G1 X2  " not in port.sent
    assert (data.uploadFlag, data.gcodeIndex) == (2, 1)


def test_a_queued_command_with_an_unclosed_comment_is_refused(data, serial):
    def queue_commands(port):
        drain(data.gcode_queue)
        data.gcode_queue.put("G0 X1 (note")
        data.gcode_queue.put("G0 X2 (closed) ; and done (")

    port = serial.run(on_poll=once_after(serial.clock, 0, queue_commands))

    assert port.sent == ["G0 X2   "]
    assert ("Command Unclosed Comment", "That command has an unclosed comment, which hangs the controller.") in alerts(serial)


def test_realtime_and_control_characters_are_removed_from_streamed_lines(data, serial):
    port = stream(data, serial, ["G1 X1! Y2~ Z`3? F4\x18\x00 "])

    assert port.sent == ["G1 X1 Y2 Z3 F4  "]


def test_realtime_and_control_characters_are_removed_from_queued_commands(data, serial):
    def queue_commands(port):
        drain(data.gcode_queue)
        data.gcode_queue.put("G0 X1!~`?\x18\x7f")

    port = serial.run(on_poll=once_after(serial.clock, 0, queue_commands))

    assert port.sent == ["G0 X1 "]


def test_realtime_commands_sent_on_purpose_still_go_out(data, serial):
    def queue_commands(port):
        drain(data.gcode_queue)
        data.quick_queue.put("~")

    port = serial.run(on_poll=once_after(serial.clock, 0, queue_commands))

    assert "~" in port.sent


STATUS = "<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>\r\n"


def every_poll(*hooks):
    return lambda port: [hook(port) for hook in hooks]


def status_reports_until(clock, end):
    """Sends a status report every 0.2 s of fake time, as the firmware does, until `end` seconds from now."""
    stop_at = clock.now + end
    state = {"next": clock.now}

    def hook(port):
        if state["next"] <= clock.now < stop_at:
            port.incoming.append(STATUS)
            state["next"] = clock.now + 0.2

    return hook


def sent_after(port, line):
    return port.sent[port.sent.index(line) + 1:]


@pytest.mark.parametrize("halt", [
    "ALARM: The sled is not keeping up with its expected position and has halted.\r\n",
    "Buffer overflow!\r\n",
])
def test_manual_commands_go_out_after_the_controller_halts_itself(data, serial, halt):
    data.gcode = [f"G1 X{n} " for n in range(50)]
    data.uploadFlag = 1

    def stop_acking(port):
        port.ack = False

    def controller_halts(port):
        port.incoming.append(halt)
        data.uploadFlag = 0  # as MessageProcessor does on this line
        data.gcode_queue.put("G0 X99")

    port = serial.run(buffer=True, on_poll=every_poll(
        once_after(serial.clock, 0, lambda port: drain(data.gcode_queue)),
        once_after(serial.clock, 0.5, stop_acking),
        once_after(serial.clock, 1, controller_halts),
        status_reports_until(serial.clock, 3),
    ))

    assert port.sent[-1] == "G0 X99 "


def test_a_stale_ok_after_a_stop_does_not_let_two_unacked_lines_into_the_controller(data, serial):
    def outstanding_line(port):
        port.ack = False
        data.gcode_queue.put("G0 X0")

    def late_oks(port):
        port.incoming.extend(["ok\r\n", "ok\r\n"])  # the outstanding line's ack, then the stop's
        data.gcode_queue.put("G0 X1")
        data.gcode_queue.put("G0 X2")

    port = serial.run(on_poll=every_poll(
        once_after(serial.clock, 0, lambda port: drain(data.gcode_queue)),
        once_after(serial.clock, 0.5, outstanding_line),
        once_after(serial.clock, 1, lambda port: data.quick_queue.put("!")),
        once_after(serial.clock, 1.05, late_oks),
        status_reports_until(serial.clock, 3),
    ))

    assert sent_after(port, "!") == ["G0 X1 "]


def test_a_resume_sent_while_resyncing_after_a_stop_does_not_block_manual_commands(data, serial):
    def stop_then_resume(port):
        port.ack = False
        data.quick_queue.put("!")
        data.quick_queue.put("~")
        data.gcode_queue.put("G0 X1")

    port = serial.run(on_poll=every_poll(
        once_after(serial.clock, 0, lambda port: drain(data.gcode_queue)),
        once_after(serial.clock, 1, stop_then_resume),
        status_reports_until(serial.clock, 3),
    ))

    assert sent_after(port, "!") == ["~", "G0 X1 "]


@pytest.mark.parametrize("halt", ["ALARM: The sled is not keeping up\r\n", "Buffer overflow!\r\n"])
def test_a_halt_mid_cut_rewinds_the_index_to_the_oldest_line_the_controller_dropped(data, serial, halt):
    data.gcode = [f"G1 X{n} " for n in range(50)]
    data.uploadFlag = 1
    data.jobInProgress = True
    first_unacked = []

    def stop_acking(port):
        port.ack = False
        first_unacked.append(data.gcodeIndex)

    def controller_halts(port):
        port.incoming.append(halt)
        data.uploadFlag = 0  # as MessageProcessor does on this line

    serial.run(buffer=True, on_poll=every_poll(
        once_after(serial.clock, 0, lambda port: drain(data.gcode_queue)),
        once_after(serial.clock, 0.5, stop_acking),
        once_after(serial.clock, 1, controller_halts),
    ))

    assert first_unacked[0] > 0
    assert data.gcodeIndex == first_unacked[0]


def test_a_halt_outside_a_job_leaves_the_index_alone(data, serial):
    data.gcode = [f"G1 X{n} " for n in range(50)]
    data.gcodeIndex = 7
    data.jobInProgress = False

    def controller_halts(port):
        drain(data.gcode_queue)
        port.ack = False
        data.gcode_queue.put("G0 X1")
        port.incoming.append("ALARM: The sled is not keeping up\r\n")

    serial.run(on_poll=once_after(serial.clock, 0, controller_halts))

    assert data.gcodeIndex == 7


def test_a_status_report_still_queued_from_before_a_stop_does_not_end_the_resync(data, serial):
    """A slow host reads reports the firmware sent before the stop well after the resync delay."""
    PE = "[PE:0.00,0.00,126,0.00,0.00]\r\n"

    def outstanding_line(port):
        port.ack = False
        data.gcode_queue.put("G0 X0")

    def stop_behind_a_backlog(port):
        port.incoming.extend([PE] * 40 + [STATUS, "ok\r\n", "ok\r\n"])  # the outstanding line's ack, then the stop's
        data.quick_queue.put("!")
        data.gcode_queue.put("G0 X1")
        data.gcode_queue.put("G0 X2")

    port = serial.run(on_poll=every_poll(
        once_after(serial.clock, 0, lambda port: drain(data.gcode_queue)),
        once_after(serial.clock, 0.5, outstanding_line),
        once_after(serial.clock, 1, stop_behind_a_backlog),
        once_after(serial.clock, 2, lambda port: port.incoming.append(STATUS)),
    ))

    assert sent_after(port, "!") == ["G0 X1 "]
