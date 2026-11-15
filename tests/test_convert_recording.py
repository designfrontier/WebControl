"""tools/convert_recording.py turns a recording into a transcript skeleton, and a recorded session replays."""
import sys

import jsonschema
import pytest

import serial_contract
from DataStructures.serialRecorder import SerialRecorder

sys.path.insert(0, str(serial_contract.REPO / "tools"))
import convert_recording  # noqa: E402

SCHEMA = serial_contract.load(serial_contract.SCHEMA)
SESSION = {
    "type": "session", "t": 0, "started": "2026-11-14T09:30:00", "port": "/dev/ttyACM0", "units": "INCHES",
    "home": {"x": 1.5, "y": -2.0}, "webcontrol": "1.27",
    "settings": {"Maslow Settings": {"bufferOn": 1, "bedWidth": "2438.4"}, "Advanced Settings": {"truncate": 0}},
}


def convert(records, **kwargs):
    warnings = []
    transcript = convert_recording.convert(records, "converted", "test.jsonl", warn=warnings.append, **kwargs)
    jsonschema.validate(transcript, SCHEMA)
    return transcript, warnings


def test_the_session_line_becomes_the_setup_with_only_changed_settings():
    transcript, _ = convert([SESSION], gcode_path="/somewhere/part.nc")

    assert transcript["setup"] == {
        "start": "boot", "units": "INCHES", "home": {"x": 1.5, "y": -2.0},
        "settings": {"Maslow Settings": {"bufferOn": 1}}, "gcode": {"path": "gcode/part.nc"},
    }
    assert transcript["meta"]["webcontrolCommit"] == "WebControl 1.27"


def test_lines_become_events_with_ticks_for_the_gaps():
    records = [
        SESSION,
        {"type": "rx", "t": 0.1, "data": "ok\r\n"},
        {"type": "ui", "t": 5.0, "seq": 1, "command": "stopRun", "arg": "", "arg1": ""},
        {"type": "tx", "t": 5.05, "data": "!\n", "cause": 1},
        {"type": "rx", "t": 5.1, "data": "ok\r\n"},
        {"type": "rx", "t": 5.5, "data": "<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>\r\n"},
        {"type": "tx", "t": 15.0, "data": "G0 X1 \n"},
    ]

    events = convert(records)[0]["events"]

    assert events == [
        {"type": "rx", "line": "ok"},
        {"type": "tick", "seconds": 4.5},
        {"type": "ui", "action": {"command": "stopRun", "arg": "", "arg1": ""}},
        {"type": "tx", "bytes": "!\n", "realtime": True},
        {"type": "rx", "line": "ok"},
        {"type": "tick", "seconds": 0.4},
        {"type": "rx", "line": "<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>"},
        {"type": "tick", "seconds": 4.5},
        {"type": "tx", "bytes": "G0 X1 \n",
         "comment": "TODO: no recorded action explains this write; add the ui event that caused it"},
    ]


def test_status_reports_can_be_left_out_and_drops_are_marked():
    records = [
        SESSION,
        {"type": "rx", "t": 0.1, "data": "<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>\r\n"},
        {"type": "rx", "t": 0.15, "data": "[PE:0.00,0.00,127]\r\n"},
        {"type": "dropped", "t": 0.2, "count": 3},
        {"type": "rx", "t": 0.2, "data": "ok\r\n"},
    ]

    events, warnings = convert(records, drop_status=True)

    assert events["events"] == [{"type": "rx", "line": "ok", "comment": "the recorder dropped 3 records here"}]
    assert warnings == ["the recorder dropped 3 records here (t=0.20)"]


@pytest.mark.parametrize("version,notice,expected", [
    ("51.27", False, {"variant": "holey", "version": "51.27"}),
    ("51.29", True, {"variant": "holey", "version": "51.29", "build": "51.292"}),
    ("1.26", False, {"variant": "stock", "version": "1.26"}),
])
def test_the_firmware_comes_from_the_b05_reply(version, notice, expected):
    records = [SESSION, {"type": "rx", "t": 1, "data": f"Firmware Version {version}\r\n"}]
    records += [{"type": "rx", "t": 2, "data": "Change in setting, rewriting EEPROM\r\n"}] if notice else []

    assert convert(records)[0]["meta"]["firmware"] == expected


def test_names_are_made_safe_for_the_schema():
    assert convert_recording.transcript_name("recording_20261114-093000") == "recording_20261114_093000"


def record(app, data, tmp_path, monkeypatch, transcript):
    setup = {**transcript["setup"],
             "settings": {**transcript["setup"].get("settings", {}), "WebControl Settings": {"recordSerial": 1}}}
    with serial_contract.Engine(app, setup, None, "replay", transcript["meta"]["firmware"]) as engine:
        recorder = SerialRecorder(tmp_path, clock=engine.clock.time)
        recorder.setEnabled(True)
        monkeypatch.setattr(data, "serialRecorder", recorder)
        engine.run(transcript["events"])
    recorder.flush()
    return convert_recording.read_records(sorted(tmp_path.glob("*.jsonl")))


@pytest.mark.parametrize("name", [
    "stream_unbuffered_with_home_shift", "stop_mid_line_resets_the_index", "alarm_mid_cut_rewinds_and_resumes",
    "define_home_from_sled_then_home",
])
def test_a_recorded_session_converts_to_a_transcript_that_replays(app, data, tmp_path, monkeypatch, name):
    transcript = serial_contract.load(serial_contract.CONTRACT / "transcripts" / (name + ".json"))
    records = record(app, data, tmp_path, monkeypatch, transcript)
    gcode = transcript["setup"].get("gcode", {}).get("path")

    converted, warnings = convert(records, gcode_path=gcode)

    assert warnings == []
    assert converted["setup"].get("gcode") == transcript["setup"].get("gcode")
    serial_contract.replay(app, converted)
