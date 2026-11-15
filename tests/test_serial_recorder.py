"""The opt-in serial recorder: what it writes, that it never blocks or breaks the serial thread, and its setting."""
import datetime
import json

import jsonschema
import pytest

import serial_contract
from DataStructures.serialRecorder import SerialRecorder
from helpers import drain

RECORDING_SCHEMA = serial_contract.load(serial_contract.CONTRACT / "recording.schema.json")
WALL = datetime.datetime(2026, 11, 14, 9, 30, 0)


class Tick:
    def __init__(self):
        self.now = 50.0

    def __call__(self):
        self.now += 0.5
        return self.now


def read(directory):
    return [json.loads(line) for path in sorted(directory.glob("*.jsonl")) for line in path.read_text().splitlines()]


def recorder(tmp_path, **kwargs):
    made = SerialRecorder(tmp_path / "rec", clock=Tick(), wallClock=lambda: WALL, **kwargs)
    made.setEnabled(True)
    return made


def test_nothing_is_written_while_disabled(tmp_path):
    made = recorder(tmp_path)
    made.setEnabled(False)

    made.startSession("/dev/ttyACM0", "MM", {"x": 0, "y": 0}, {}, "1.27")
    made.tx("G21 \n")
    made.flush()

    assert not (tmp_path / "rec").exists()
    assert made.writer is None


def test_records_are_written_in_order_with_session_times_and_causes(tmp_path):
    made = recorder(tmp_path)

    made.startSession("/dev/ttyACM0", "MM", {"x": 1.5, "y": 0}, {"Maslow Settings": {"bufferOn": 0}}, "1.27")
    made.action("move", "right", "10")
    made.tx("G91  \n")
    made.rx("ok\r\n")
    made.gcodeLoaded("/tmp/a.nc", 12)
    made.flush()

    records = read(tmp_path / "rec")
    assert [record["type"] for record in records] == ["session", "ui", "tx", "rx", "gcode"]
    assert records[0]["started"] == "2026-11-14T09:30:00" and records[0]["t"] == 0
    assert records[2] == {"type": "tx", "data": "G91  \n", "cause": 1, "t": 1.0}
    assert records[3]["data"] == "ok\r\n"
    assert [path.name for path in (tmp_path / "rec").iterdir()] == ["20261114-093000.jsonl"]
    [jsonschema.validate(record, RECORDING_SCHEMA) for record in records]


def test_a_full_queue_drops_records_and_counts_them(tmp_path, monkeypatch):
    made = recorder(tmp_path, queueLimit=2)
    monkeypatch.setattr(made, "_ensureWriter", lambda: None)

    [made.tx(f"G0 X{n} \n") for n in range(5)]
    assert made.dropped == 3
    [made.write(made.queue.get_nowait()) for _ in range(2)]
    made.file.flush()

    records = read(tmp_path / "rec")
    assert [record["type"] for record in records] == ["dropped", "tx", "tx"]
    assert records[0]["count"] == 3
    jsonschema.validate(records[0], RECORDING_SCHEMA)


def test_a_failing_disk_never_reaches_the_caller(tmp_path):
    blocker = tmp_path / "rec"
    blocker.write_text("a file where the directory should be")
    made = SerialRecorder(blocker, clock=Tick(), wallClock=lambda: WALL)
    made.setEnabled(True)

    made.tx("G21 \n")
    made.flush()

    assert made.queue.unfinished_tasks == 0


def test_files_rotate_by_size_and_only_the_newest_are_kept(tmp_path):
    made = recorder(tmp_path, maxFileBytes=200, maxFiles=3)

    made.startSession("/dev/ttyACM0", "MM", {"x": 0, "y": 0}, {}, "1.27")
    [made.tx(f"G1 X{n} Y{n} F800  \n") for n in range(40)]
    made.flush()

    names = sorted(path.name for path in (tmp_path / "rec").iterdir())
    assert len(names) == 3
    assert all(name.startswith("20261114-093000.") for name in names)
    assert all((tmp_path / "rec" / name).stat().st_size < 400 for name in names)


def test_a_second_session_in_the_same_second_gets_its_own_file(tmp_path):
    made = recorder(tmp_path)

    made.startSession("/dev/ttyACM0", "MM", {"x": 0, "y": 0}, {}, "1.27")
    made.flush()
    made.startSession("/dev/ttyACM0", "MM", {"x": 0, "y": 0}, {}, "1.27")
    made.flush()

    assert sorted(path.name for path in (tmp_path / "rec").iterdir()) == [
        "20261114-093000-1.jsonl", "20261114-093000.jsonl",
    ]


@pytest.fixture
def recording(app, data, tmp_path, monkeypatch):
    def start(engine):
        made = SerialRecorder(tmp_path / "rec", clock=engine.clock.time, wallClock=lambda: WALL)
        made.setEnabled(True)
        monkeypatch.setattr(data, "serialRecorder", made)
        return made

    return start


def test_the_backend_records_every_line_it_writes_and_reads_and_each_action(app, recording, tmp_path):
    transcript = serial_contract.load(serial_contract.CONTRACT / "transcripts" / "connect_boot_handshake.json")
    events = transcript["events"] + [
        {"type": "ui", "action": {"command": "move", "arg": "right", "arg1": "10"}},
        {"type": "tx", "bytes": "G91  \n"}, {"type": "rx", "line": "ok"},
        {"type": "tx", "bytes": "G00 X10.0  \n"}, {"type": "rx", "line": "ok"},
        {"type": "tx", "bytes": "G90  \n"}, {"type": "rx", "line": "ok"},
    ]
    setup = {**transcript["setup"], "settings": {"WebControl Settings": {"recordSerial": 1}}}
    with serial_contract.Engine(app, setup, None, "replay", transcript["meta"]["firmware"]) as engine:
        made = recording(engine)
        engine.run(events)
    made.flush()

    records = read(tmp_path / "rec")
    assert records[0]["type"] == "session" and records[0]["units"] == "MM"
    assert [r["data"] for r in records if r["type"] == "tx"] == [e["bytes"] for e in events if e["type"] == "tx"]
    assert [r["data"] for r in records if r["type"] == "rx"] == [e["line"] + "\r\n" for e in events if e["type"] == "rx"]
    move = next(r for r in records if r["type"] == "ui")
    assert move["command"] == "move"
    assert [r.get("cause") for r in records if r["type"] == "tx"][-3:] == [move["seq"]] * 3


def test_the_setting_is_off_by_default_and_listed_on_the_webcontrol_tab(client, data):
    body = client.get("/api/settings/webcontrol").get_json()
    record = next(item for item in body["settings"] if item["key"] == "recordSerial")

    assert (record["title"], record["type"], record["value"]) == ("Record serial traffic", "bool", 0)
    assert "serial-recordings" in record["desc"]


def test_saving_the_setting_turns_recording_on_and_off(client, data, monkeypatch):
    monkeypatch.setattr(data.config, "saveSettings", lambda: None)
    monkeypatch.setattr(data, "serialRecorder", SerialRecorder())
    try:
        client.put("/api/settings/webcontrol", json={"values": {"recordSerial": True}})
        assert data.serialRecorder.enabled is True
        assert data.config.getValue("WebControl Settings", "recordSerial") == 1

        client.put("/api/settings/webcontrol", json={"values": {"recordSerial": False}})
        assert data.serialRecorder.enabled is False
    finally:
        data.config.setValue("WebControl Settings", "recordSerial", 0)
        drain(data.gcode_queue)
