"""The opt-in serial recorder: what it writes, that it never blocks or breaks the serial thread, and its setting."""
import datetime
import json
import subprocess
import sys
import textwrap
from pathlib import Path

import jsonschema
import pytest

import serial_contract
from DataStructures.serialRecorder import SerialRecorder
from helpers import drain

RECORDING_SCHEMA = serial_contract.load(serial_contract.CONTRACT / "recording.schema.json")
WALL = datetime.datetime(2026, 11, 14, 9, 30, 0)
ROOT = Path(__file__).resolve().parent.parent


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
    assert made.writerRunning is False


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


def test_a_full_queue_drops_records_and_notes_where(tmp_path, monkeypatch):
    made = recorder(tmp_path, queueLimit=3)
    monkeypatch.setattr(made, "_ensureWriter", lambda: None)

    made.startSession("/dev/ttyACM0", "MM", {"x": 0, "y": 0}, {}, "1.27")
    [made.tx(f"G0 X{n} \n") for n in range(5)]
    assert made.dropped == 3
    made.queue.popleft()
    made.tx("G0 X9 \n")
    made._drain()

    records = read(tmp_path / "rec")
    assert [record["type"] for record in records] == ["tx", "tx", "dropped", "tx"]
    assert records[2]["count"] == 3 and records[2]["t"] == records[3]["t"]
    assert records[3]["data"] == "G0 X9 \n"
    jsonschema.validate(records[2], RECORDING_SCHEMA)


def test_records_before_the_first_session_are_not_kept(tmp_path):
    made = recorder(tmp_path)

    made.gcodeLoaded("/tmp/a.nc", 12)
    made.action("move", "right", "10")
    made.startSession("/dev/ttyACM0", "MM", {"x": 0, "y": 0}, {}, "1.27", gcode="/tmp/a.nc")
    made.flush()

    records = read(tmp_path / "rec")
    assert [record["type"] for record in records] == ["session"]
    assert records[0]["gcode"] == "/tmp/a.nc"
    jsonschema.validate(records[0], RECORDING_SCHEMA)


def test_a_failing_disk_never_reaches_the_caller_and_is_reported_once(tmp_path, capsys):
    blocker = tmp_path / "rec"
    blocker.write_text("a file where the directory should be")
    made = SerialRecorder(blocker, clock=Tick(), wallClock=lambda: WALL)
    made.setEnabled(True)

    made.startSession("/dev/ttyACM0", "MM", {"x": 0, "y": 0}, {}, "1.27")
    [made.tx("G21 \n") for _ in range(50)]
    made.flush()

    assert not made.queue
    assert capsys.readouterr().out.count("serial recording stopped") == 1


def test_recording_resumes_at_the_next_session_after_a_failure(tmp_path):
    made = recorder(tmp_path)
    made.startSession("/dev/ttyACM0", "MM", {"x": 0, "y": 0}, {}, "1.27")
    made.flush()
    made.file.close()
    made.tx("lost \n")
    made.flush()

    made.startSession("/dev/ttyACM0", "MM", {"x": 0, "y": 0}, {}, "1.27")
    made.tx("G21 \n")
    made.flush()

    second = (tmp_path / "rec" / "20261114-093000-1.jsonl").read_text().splitlines()
    assert [json.loads(line).get("data") for line in second] == [None, "G21 \n"]
    assert "lost" not in (tmp_path / "rec" / "20261114-093000.jsonl").read_text()


def test_files_rotate_by_size_and_keep_the_session_header(tmp_path):
    made = recorder(tmp_path, maxFileBytes=200, maxFiles=3)

    made.startSession("/dev/ttyACM0", "MM", {"x": 0, "y": 0}, {}, "1.27")
    [made.tx(f"G1 X{n} Y{n} F800  \n") for n in range(40)]
    made.flush()

    names = sorted(path.name for path in (tmp_path / "rec").iterdir())
    assert len(names) == 3
    assert "20261114-093000.jsonl" in names
    assert all(name.startswith("20261114-093000.") for name in names)
    assert all((tmp_path / "rec" / name).stat().st_size < 400 for name in names)
    assert json.loads((tmp_path / "rec" / "20261114-093000.jsonl").read_text().splitlines()[0])["type"] == "session"


def test_a_slow_disk_does_not_stall_the_gevent_hub(tmp_path):
    script = textwrap.dedent('''
        from gevent import monkey
        monkey.patch_all()
        import sys, time, gevent
        sys.path.insert(0, %r)
        from DataStructures import serialRecorder
        made = serialRecorder.SerialRecorder(%r)
        made.setEnabled(True)
        realWrite = made._writeLine
        def slowWrite(record):
            monkey.get_original("time", "sleep")(0.3)
            realWrite(record)
        made._writeLine = slowWrite
        made.startSession("p", "MM", {"x": 0, "y": 0}, {}, "1.27")
        gaps, last = [], time.monotonic()
        for n in range(60):
            made.tx("G0 X%%d \\n" %% n)
            gevent.sleep(0.01)
            now = time.monotonic()
            gaps.append(now - last)
            last = now
        print(max(gaps))
    ''') % (str(ROOT), str(tmp_path / "rec"))
    result = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True, timeout=60)

    assert result.returncode == 0, result.stderr
    assert float(result.stdout.split()[-1]) < 0.15


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
