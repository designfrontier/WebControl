"""Replays serial transcripts (contract/serial) against the real backend, or records new ones, on a fake port and clock.

The serial thread's own read/write loop runs unchanged; each pass calls the port's `in_waiting`, where `Engine.poll`
feeds controller lines, performs UI actions and checks what the backend wrote.
"""
import copy
import json
import queue
import re
import tempfile
import threading
import time as real_time
import types
from collections import deque
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CONTRACT = REPO / "contract" / "serial"
SCHEMA = CONTRACT / "transcript.schema.json"
NAMESPACE = "/MaslowCNC"
FORMAT_VERSION = 1

TIMING = {"quiet": 0.1, "txTimeout": 2.0, "settle": 1.0}
BOOT_BANNER = ["PCB v1.6 TLE9201  Detected", "FAKE_SERVO off", "Grbl v1.00", "ready", "ok"]
EEPROM_REWRITE_NOTICE = "Change in setting, rewriting EEPROM"
REALTIME = ("!", "~")
# The whole run, prelude included, may not take longer than this on the fake clock.
FAKE_TIME_LIMIT = 600.0
# Bundled-firmware facts and process identity, which a transcript's setup never changes.
KEEP_DATA = {
    "version", "stockFirmwareVersion", "customFirmwareVersion", "holeyFirmwareVersion", "lastChecked", "comPorts",
    "comport", "hostAddress", "platform", "platformHome", "clients",
}
SIMPLE = (bool, int, float, str, type(None), list, tuple, dict)


class ContractFailure(AssertionError):
    pass


class Clock:
    def __init__(self):
        self.now = 1000.0

    def time(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


class OwnedQueue(queue.Queue):
    """A queue only the engine's thread can see items in, so main.py's MessageProcessor thread never takes them."""

    def __init__(self):
        super().__init__()
        self.owner = threading.get_ident()

    def empty(self):
        return threading.get_ident() != self.owner or super().empty()


class FakePort:
    """
    The byte stream the backend writes, cut into lines after each newline however the writes were split.
    """

    def __init__(self, on_poll, clock):
        self.on_poll = on_poll
        self.clock = clock
        self.incoming = deque()
        self.written = []
        self.written_at = []
        self.partial = ""
        self.is_open = True
        self.parity = None

    def open(self):
        self.is_open = True

    def close(self):
        self.is_open = False

    @property
    def in_waiting(self):
        self.on_poll(self)
        if not self.is_open:
            raise OSError("Attempting to use a port that is not open")
        return len(self.incoming)

    def readline(self):
        return self.incoming.popleft().encode()

    def write(self, payload):
        if not self.is_open:
            raise OSError("Attempting to use a port that is not open")
        self.partial += payload.decode("utf-8", "backslashreplace")
        *lines, self.partial = self.partial.split("\n")
        self.written.extend(line + "\n" for line in lines)
        self.written_at.extend(self.clock.now for _ in lines)


class Firmware:
    """
    A controller model for the prelude and the recorder: answers each line with `ok` in order, holds the line a rule
    marks as pausing until `~`, and flushes on `!`, as spec/firmware.md describes holey 51.27.
    """

    def __init__(self, version="51.27", build=None):
        self.ack = True
        self.pending = deque()
        self.held = False
        self.seen = 0
        self.rules = [(re.compile(r"^B05\b"), ["Firmware Version " + version], False)]
        if build is not None and build.startswith("51.29") and build != "51.29x":
            self.rules.append((re.compile(r"^\$\d+="), setting_write_replies, False))

    def add_rule(self, pattern, replies, hold=False):
        self.rules.insert(0, (re.compile(pattern), replies if callable(replies) else list(replies), hold))

    def hold_unanswered(self, line):
        self.pending.append(line.rstrip("\n"))
        self.seen += 1

    def flush(self):
        self.pending.clear()
        self.held = False

    def respond(self, written):
        '''
        :return: the lines the controller sends for everything written since the last call.
        '''
        out = []
        for line in written[self.seen:]:
            text = line.rstrip("\n")
            if text == "!":
                self.flush()
                out.append("ok")
            elif text == "~":
                out.append("ok")
                if self.held:
                    self.held = False
                    self.pending.popleft()
                    out.append("ok")
            else:
                self.pending.append(text)
        self.seen = len(written)
        return out + self._run()

    def _run(self):
        out = []
        while self.ack and not self.held and self.pending:
            line = self.pending[0]
            replies, hold = next(((replies, hold) for pattern, replies, hold in self.rules
                                  if pattern.search(line.strip())), ([], False))
            out.extend(replies(line.strip()) if callable(replies) else replies)
            if hold:
                self.held = True
                break
            self.pending.popleft()
            out.append("ok")
        return out


def setting_write_replies(line):
    '''
    What 51.292 prints for `$n=v` before its ok (System.cpp and Settings.cpp at v51.292); floats print with 2 decimals.
    '''
    key, value = re.match(r"^\$(\d+)=(\S+)", line).groups()
    echo = "%.2f" % float(value)
    limit = {"63": ["", "new zaxis upper limit=" + echo], "64": ["", "new zaxis lower limit=" + echo]}
    return ["the value received: " + echo, *limit.get(key, []), EEPROM_REWRITE_NOTICE]


def load(path):
    with open(path) as handle:
        return json.load(handle)


def transcripts():
    return sorted((CONTRACT / "transcripts").glob("*.json"))


def gcode_text(setup):
    gcode = setup.get("gcode")
    if gcode is None:
        return None
    if "path" in gcode:
        return (CONTRACT / gcode["path"]).read_text()
    return "\n".join(gcode["lines"]) + "\n"


def decode_envelope(envelope):
    data = envelope.get("data")
    if envelope.get("dataFormat") == "json" and isinstance(data, str):
        try:
            data = json.loads(data)
        except ValueError:
            pass
    return {"command": envelope.get("command"), "data": data}


def matches(expected, actual):
    '''
    Strings match as substrings, objects field by field, everything else by equality.
    '''
    if isinstance(expected, dict):
        return isinstance(actual, dict) and all(key in actual and matches(value, actual[key])
                                                for key, value in expected.items())
    if isinstance(expected, str) and isinstance(actual, str):
        return expected in actual
    return same(expected, actual)


def same(expected, actual):
    '''
    JSON equality as serde_json would see it: numbers as f64, booleans never equal to numbers.
    '''
    if isinstance(expected, bool) or isinstance(actual, bool):
        return type(expected) is type(actual) and expected == actual
    if isinstance(expected, (int, float)) and isinstance(actual, (int, float)):
        return float(expected) == float(actual)
    if isinstance(expected, dict) and isinstance(actual, dict):
        return expected.keys() == actual.keys() and all(same(value, actual[key]) for key, value in expected.items())
    if isinstance(expected, list) and isinstance(actual, list):
        return len(expected) == len(actual) and all(map(same, expected, actual))
    return expected == actual


def alert_as_emitted(alert):
    return {"command": "activateModal", "data": {**alert, "modalType": "alert"}}


class Engine:
    """
    Runs one transcript or scenario. In "replay" mode tx events are checked against the port; in "record" mode every
    write becomes a tx event in `recorded`.
    """

    def __init__(self, app, setup, timing=None, mode="replay", firmware=None):
        self.app = app
        self.data = app.data
        self.setup = setup
        self.timing = {**TIMING, **(timing or {})}
        self.mode = mode
        self.meta_firmware = firmware or {"variant": "holey", "version": "51.27"}
        self.clock = Clock()
        self.firmware = Firmware(self.meta_firmware["version"], self.meta_firmware.get("build"))
        self.recorded = []
        self.emitted = []
        self.emit_mark = 0
        self.cursor = 0
        self.failure = None
        self.finished = False
        self.current = None
        self.phase = None
        self.settle_start = None
        self.wait_until = None
        self.tx_deadline = None
        self.in_prelude = False
        self.index = -1
        self.port = None

    # ----- setup and teardown -----

    def __enter__(self):
        import Background.messageProcessor as message_processor
        import Background.UIProcessor as ui_processor
        import Connection.serialPortThread as serial_port_thread
        import main
        from Background.messageProcessor import MessageProcessor
        from Background.UIProcessor import UIProcessor
        from pytest import MonkeyPatch

        self.patch = MonkeyPatch()
        data = self.data
        self.saved_data = {name: copy.deepcopy(getattr(data, name)) for name in self._simple_data()}
        self.saved_settings = copy.deepcopy(data.config.settings)
        self.saved_request = data.serialPort.serialPortRequest
        self.tempdir = tempfile.TemporaryDirectory(prefix="serial-contract-")
        clock = self.clock
        self.patch.setattr(serial_port_thread, "time", clock)
        self.patch.setattr(serial_port_thread, "serial", types.SimpleNamespace(
            VERSION="3.5", PARITY_ODD="O", PARITY_NONE="N", Serial=lambda *args, **kwargs: self.port,
        ))
        # main.py's own MessageProcessor thread shares this module, so it still needs a real sleep.
        self.patch.setattr(message_processor, "time", types.SimpleNamespace(time=clock.time, sleep=real_time.sleep))
        self.patch.setattr(ui_processor, "time", types.SimpleNamespace(time=clock.time, sleep=lambda seconds: None))
        self.patch.setattr(data.config, "saveSettings", lambda: None)
        self.patch.setattr(data, "message_queue", OwnedQueue())
        [self._drain(q) for q in (data.gcode_queue, data.quick_queue, data.ui_queue1, data.ui_controller_queue)]
        try:
            self._reset_data()
            self._apply_setup()
        except BaseException:
            self.__exit__(None, None, None)
            raise
        self.processor = MessageProcessor()
        self.processor.setUpData(data)
        self.ui = UIProcessor()
        self.ui.app = self.app
        self.socket = main.socketio.test_client(self.app, namespace=NAMESPACE)
        self.http = self.app.test_client()
        return self

    def __exit__(self, *exc):
        data = self.data
        socket = getattr(self, "socket", None)
        if socket is not None and socket.is_connected(NAMESPACE):
            socket.disconnect(NAMESPACE)
        [self._drain(q) for q in (data.gcode_queue, data.quick_queue, data.ui_queue1, data.ui_controller_queue)]
        self.patch.undo()
        for name, value in self.saved_data.items():
            setattr(data, name, value)
        data.config.settings = self.saved_settings
        data.serialPort.serialPortRequest = self.saved_request
        data.gcodeFile.filename = ""
        data.gcodeFile.line3D[:] = []
        data.holeyCalibration.hasResult = False
        self.tempdir.cleanup()
        return False

    def _simple_data(self):
        from DataStructures.data import Data

        return [name for name in dir(Data) if not name.startswith("_") and name not in KEEP_DATA
                and isinstance(getattr(Data, name), SIMPLE) and isinstance(getattr(self.data, name), SIMPLE)]

    def _reset_data(self):
        from DataStructures.data import Data

        [setattr(self.data, name, copy.deepcopy(getattr(Data, name))) for name in self._simple_data()]
        self.data.serialPort.serialPortRequest = ""
        self.data.holeyCalibration.hasResult = False

    def _option(self, section, key):
        return next(option for option in self.data.config.settings[section] if option["key"] == key)

    def _apply_setup(self):
        data, setup, config = self.data, self.setup, self.data.config
        for section, values in setup.get("settings", {}).items():
            for key, value in values.items():
                self._option(section, key)["value"] = value
        units = setup.get("units", "MM")
        data.units = units
        self._option("Computed Settings", "units")["value"] = units
        self._option("Computed Settings", "unitsZ")["value"] = setup.get("unitsZ", units)
        home = setup.get("home", {"x": 0, "y": 0})
        self._option("Advanced Settings", "homeX")["value"] = str(home["x"])
        self._option("Advanced Settings", "homeY")["value"] = str(home["y"])
        data.gcodeShift = [float(home["x"]), float(home["y"])]
        data.tolerance = 0.020 if units == "INCHES" else 0.5
        config.computeSettings(None, None, None, True)
        text = gcode_text(setup)
        if text is not None:
            path = Path(self.tempdir.name) / setup["gcode"].get("path", "inline.nc").split("/")[-1]
            path.write_text(text)
            data.gcodeFile.filename = str(path)
            data.gcodeFile.loadUpdateFile()
        [self._drain(q) for q in (data.gcode_queue, data.ui_queue1)]

    @staticmethod
    def _drain(q):
        while not q.empty():
            q.get_nowait()

    # ----- running -----

    def run(self, events):
        '''
        Runs the prelude (if the setup starts connected) and then `events`, an iterable of event dicts.
        '''
        from Connection.serialPortThread import SerialPortThread

        self.source = iter(self._with_prelude(events))
        self.port = FakePort(self.poll, self.clock)
        thread = SerialPortThread()
        thread.data = self.data
        thread.getmessage()
        if self.failure is not None:
            raise self.failure
        if not self.finished:
            raise ContractFailure(self._where() + "the backend closed the connection (no controller line for 5 s?)")
        return self.recorded

    def _with_prelude(self, events):
        if self.setup.get("start", "connected") == "connected":
            yield {"type": "_prelude"}
            for line in self.setup.get("bootBanner", BOOT_BANNER):
                yield {"type": "rx", "line": line}
            yield from self.idle()
            sled = self.setup.get("sled")
            if sled is not None:
                yield {"type": "rx", "line": status_line(sled)}
            gcode_index = self.setup.get("gcodeIndex", 0)
            if gcode_index:
                yield {"type": "ui", "action": {"command": "moveGcodeGoto", "arg": str(gcode_index), "arg1": ""}}
            yield {"type": "_begin"}
        yield from events

    def idle(self):
        '''
        Lets the controller model answer every line written until the backend has nothing left to say.
        '''
        while True:
            yield {"type": "_settle"}
            replies = self.firmware.respond(self.port.written)
            if not replies:
                return
            for line in replies:
                yield {"type": "rx", "line": line}

    def wait_tx(self, pattern):
        '''
        Answers lines until the backend writes one matching `pattern`, which is left unanswered.
        '''
        regex = re.compile(pattern)
        while True:
            yield {"type": "_settle"}
            written = self.port.written
            hit = next((n for n in range(self.firmware.seen, len(written)) if regex.search(written[n])), None)
            replies = self.firmware.respond(written if hit is None else written[:hit])
            if hit is None and not replies:
                raise ContractFailure("the backend never wrote a line matching " + repr(pattern))
            for line in replies:
                yield {"type": "rx", "line": line}
            if hit is not None:
                self.firmware.hold_unanswered(written[hit])
                return

    def poll(self, port):
        if self.finished:
            return
        try:
            self._process_backend()
            while self._advance():
                pass
            if self.clock.now > 1000.0 + FAKE_TIME_LIMIT:
                raise ContractFailure(self._where() + "ran past the fake-time limit")
        except Exception as error:  # the serial loop swallows exceptions, so keep it and stop
            self.failure = error if isinstance(error, ContractFailure) else ContractFailure(
                self._where() + type(error).__name__ + ": " + str(error))
            self._finish()

    def _finish(self):
        self.finished = True
        self.data.requestSerialClose = True

    def _process_backend(self):
        while not self.data.message_queue.empty():
            self.processor.processMessage(self.data.message_queue.get_nowait())
        self._ui_pass()

    def _ui_pass(self):
        with self.app.app_context():
            self.ui.processPass()
        packets = self.socket.get_received(NAMESPACE)
        self.emitted.extend(decode_envelope(packet["args"][0] if isinstance(packet["args"], list) else packet["args"])
                            for packet in packets if packet["name"] == "message")

    def _rx_consumed(self):
        return not self.port.incoming and self.data.message_queue.qsize() == 0

    def _where(self):
        return "event " + str(self.index) + " " + json.dumps(self.current) + ": " if self.current else ""

    def _advance(self):
        if self.current is None:
            self.current = next(self.source, {"type": "_end"})
            self.index += 0 if self.current["type"].startswith("_") or self.in_prelude else 1
            self.phase = "settle"
            self.settle_start = self.wait_until = self.tx_deadline = None
        event = self.current
        if event["type"] == "tx":
            return self._match_tx(event)
        if self.phase == "settle":
            quiet = self.timing["settle"] if event["type"] == "_end" else self.timing["quiet"]
            if not self._settled(quiet):
                return False
            self.phase = "perform"
        if not self._perform(event):
            return False
        if self.mode == "record" and not self.in_prelude and not event["type"].startswith("_"):
            self.recorded.append(event)
        self.current = None
        return not self.finished

    def _settled(self, quiet):
        if not self._rx_consumed():
            self.settle_start = None
            return False
        if self.cursor < len(self.port.written):
            if self.mode == "replay" and not self.in_prelude:
                raise ContractFailure(self._where() + "unexpected tx " + json.dumps(self.port.written[self.cursor])
                                      + " (written before this event; no tx event lists it)")
            self._record_writes()
            self.settle_start = None
        if self.settle_start is None:
            self.settle_start = self.clock.now
        return self.clock.now - self.settle_start >= quiet - 1e-9

    def _record_writes(self):
        for line in self.port.written[self.cursor:]:
            if not self.in_prelude:
                self.recorded.append(tx_event(line))
        self.cursor = len(self.port.written)

    def _match_tx(self, event):
        check_realtime_flag(event)
        if self.cursor < len(self.port.written):
            actual = self.port.written[self.cursor]
            if actual != event["bytes"]:
                raise ContractFailure(self._where() + "expected tx " + json.dumps(event["bytes"]) + " but the backend wrote "
                                      + json.dumps(actual))
            if self.tx_deadline is not None and self.port.written_at[self.cursor] > self.tx_deadline + 1e-9:
                raise ContractFailure(self._where() + "the backend wrote " + json.dumps(actual) + " after the "
                                      + str(event.get("timeout", self.timing["txTimeout"])) + " s tx timeout")
            self.cursor += 1
            self.current = None
            return True
        if self.tx_deadline is None:
            self.tx_deadline = self.clock.now + event.get("timeout", self.timing["txTimeout"])
        if self.clock.now > self.tx_deadline:
            raise ContractFailure(self._where() + "the backend never wrote " + json.dumps(event["bytes"]))
        return False

    def _perform(self, event):
        kind = event["type"]
        if kind == "_prelude":
            self.in_prelude = True
        elif kind == "_begin":
            self._record_writes()
            self.in_prelude = False
            self._ui_pass()
            self.emit_mark = len(self.emitted)
        elif kind == "_end":
            if self.port.partial and self.mode == "replay":
                raise ContractFailure("the backend left " + json.dumps(self.port.partial) + " written without a newline")
            self._finish()
        elif kind == "rx":
            self.port.incoming.append(event["line"] + "\r\n")
        elif kind == "ui":
            action = event["action"]
            self.socket.emit("action", {"data": {"command": action["command"], "arg": action.get("arg", ""),
                                                 "arg1": action.get("arg1", "")}}, namespace=NAMESPACE)
        elif kind == "http":
            self._http(event)
        elif kind == "tick":
            if self.wait_until is None:
                self.wait_until = self.clock.now + event["seconds"]
            return self.clock.now >= self.wait_until - 1e-9
        elif kind == "expect":
            self._expect(event)
        elif kind != "_settle":
            raise ContractFailure(self._where() + "unknown event type " + kind)
        return True

    def _http(self, event):
        method = event.get("method", "POST")
        if "form" in event:
            response = self.http.open(event["path"], method=method, data=event["form"],
                                      content_type="multipart/form-data")
        else:
            response = self.http.open(event["path"], method=method, json=event.get("json"))
        if "status" in event and response.status_code != event["status"]:
            raise ContractFailure(self._where() + "HTTP status " + str(response.status_code))

    def _expect(self, event):
        self._ui_pass()
        problems = []
        for key, value in event.get("state", {}).items():
            actual = observed_state(self.data, key)
            if not same(value, actual):
                problems.append(key + " is " + json.dumps(actual) + ", expected " + json.dumps(value))
        for item in event.get("settings", []):
            actual = str(self._option(item["section"], item["key"])["value"])
            if actual != str(item["value"]):
                problems.append(item["key"] + " is " + json.dumps(actual) + ", expected " + json.dumps(item["value"]))
        recent = self.emitted[self.emit_mark:]
        wanted = event.get("emitted", []) + [alert_as_emitted(alert) for alert in event.get("alerts", [])]
        problems += ["nothing emitted matches " + json.dumps(item) + " among " + json.dumps(recent)
                     for item in wanted if not any(matches(item, message) for message in recent)]
        unwanted = event.get("notEmitted", []) + [alert_as_emitted(alert) for alert in event.get("noAlerts", [])]
        problems += ["emitted " + json.dumps(item) for item in unwanted
                     if any(matches(item, message) for message in recent)]
        self.emit_mark = len(self.emitted)
        if problems:
            raise ContractFailure(self._where() + "; ".join(problems))


def status_line(sled):
    return "<Idle,MPos:{:.2f},{:.2f},{:.2f},WPos:0.000,0.000,0.000>".format(sled["x"], sled["y"], sled["z"])


def tx_event(line):
    event = {"type": "tx", "bytes": line}
    return {**event, "realtime": True} if line.rstrip("\n") in REALTIME else event


def check_realtime_flag(event):
    if (event["bytes"].rstrip("\n") in REALTIME) != bool(event.get("realtime", False)):
        raise ContractFailure("tx " + json.dumps(event["bytes"]) + " must set realtime exactly when it is ! or ~")


def observed_state(data, key):
    from Connection import firmwareFacts
    from DataStructures.data import pauseButtonLabel

    readers = {
        "uploadFlag": lambda: data.uploadFlag,
        "jobInProgress": lambda: bool(data.jobInProgress),
        "gcodeIndex": lambda: data.gcodeIndex,
        "units": lambda: data.units,
        "manualZAxisAdjust": lambda: bool(data.manualZAxisAdjust),
        "pauseButtonSetting": lambda: pauseButtonLabel(data),
        "currentTool": lambda: data.currentTool,
        "positioningMode": lambda: "G91" if data.positioningMode == 1 else "G90",
        "firmwareBuild": lambda: firmwareFacts.firmwareBuild(data),
        "controllerFirmwareVersion": lambda: firmwareFacts.firmwareVersion(data),
        "controllerBoard": lambda: data.controllerBoard,
        "home": lambda: {"x": float(data.config.getValue("Advanced Settings", "homeX")),
                         "y": float(data.config.getValue("Advanced Settings", "homeY"))},
    }
    if key not in readers:
        raise ContractFailure("unknown state key " + key)
    return readers[key]()


def replay(app, transcript):
    with Engine(app, transcript["setup"], transcript.get("timing"), "replay", transcript["meta"]["firmware"]) as engine:
        engine.run(transcript["events"])


def dumps(transcript):
    '''
    Transcript JSON with one event per line, so diffs and hand-checking stay readable.
    '''
    head = {key: value for key, value in transcript.items() if key != "events"}
    text = json.dumps(head, indent=2, ensure_ascii=False)
    events = ",\n".join("    " + json.dumps(event, ensure_ascii=False) for event in transcript["events"])
    return text[:-2] + ',\n  "events": [\n' + events + "\n  ]\n}\n"
