"""tools/index_recordings.py and tools/extract_real_job.py against a small recording in v0.94-import form."""
import json
import sys

import serial_contract

sys.path.insert(0, str(serial_contract.REPO / "tools"))

import extract_real_job  # noqa: E402
import index_recordings  # noqa: E402

RECORDS = [
    {"type": "session", "t": 0, "started": "2026-01-01T00:00:00", "source": "log.txt line 1, connect 1"},
    {"type": "rx", "t": 0.5, "data": "PCB v1.1 Detected\r\n"},
    {"type": "tx", "t": 1.0, "data": "G21  \n"},
    {"type": "rx", "t": 1.1, "data": "G21\r\n"},
    {"type": "rx", "t": 1.2, "data": "ok\r\n"},
    {"type": "tx", "t": 2.0, "data": "G0 X11.5 Y22.25  \n"},
    {"type": "rx", "t": 2.1, "data": "<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>\r\n"},
    {"type": "rx", "t": 2.2, "data": "[PE:0.00,0.00,127,1.00,1.00]\r\n"},
    {"type": "rx", "t": 2.3, "data": "<Idle,MPos:1.00,0.00,0.00,WPos:0.000,0.000,0.000>\r\n"},
    {"type": "rx", "t": 2.4, "data": "[PE:0.00,0.00,127,1.00,1.00]\r\n"},
    {"type": "rx", "t": 2.5, "data": "<Idle,MPos:2.00,0.00,0.00,WPos:0.000,0.000,0.000>\r\n"},
    {"type": "rx", "t": 2.6, "data": "[PE:0.00,0.00,127,1.00,1.00]\r\n"},
    {"type": "rx", "t": 2.7, "data": "<Idle,MPos:3.00,0.00,0.00,WPos:0.000,0.000,0.000>\r\n"},
    {"type": "rx", "t": 2.8, "data": "[PE:0.00,0.00,127,1.00,1.00]\r\n"},
    {"type": "rx", "t": 2.9, "data": "<Idle,MPos:4.00,0.00,0.00,WPos:0.000,0.000,0.000>\r\n"},
    {"type": "rx", "t": 3.0, "data": "[PE:0.00,0.00,127,1.00,1.00]\r\n"},
    {"type": "rx", "t": 3.1, "data": "ok\r\n"},
    {"type": "tx", "t": 4.0, "data": "T2 M6  \n"},
    {"type": "rx", "t": 4.1, "data": "Tool Change: Please insert tool 2\r\n"},
    {"type": "tx", "t": 5.0, "data": "~\n"},
    {"type": "tx", "t": 9.0, "data": "G1 Z-1.0  \n"},
]
GCODE = ["(job)", "G21", "G0 X10 Y20", "T2 M6", "G1 Z-1"]


def recording(tmp_path):
    path = tmp_path / "20260101-000000.jsonl"
    path.write_text("".join(json.dumps(record) + "\n" for record in RECORDS))
    return path


def test_a_recording_is_summarised_with_the_first_time_of_each_feature(tmp_path):
    summary = index_recordings.summarise(str(recording(tmp_path)))
    assert (summary["tx"], summary["rx"], summary["first"], summary["last"]) == (5, 15, 0.5, 9.0)
    assert summary["source"] == "log.txt line 1, connect 1"
    features = summary["features"]
    assert features["bootBanner"] == {"count": 1, "firstT": 0.5}
    assert features["toolChange"] == {"count": 1, "firstT": 4.1}
    assert features["toolChangeM6"] == {"count": 1, "firstT": 4.0}
    assert features["resume"] == {"count": 1, "firstT": 5.0}
    assert features["ok"]["count"] == 2 and features["status"]["count"] == 5
    assert "alarm" not in features


def test_writes_in_the_windows_are_aligned_to_the_file_and_their_home_found(tmp_path):
    windows = [[0.9, 5.5, 0]]
    entries = extract_real_job.read_windows(recording(tmp_path), windows)
    assert [entry["tx"] for entry in entries] == ["G21  \n", "G0 X11.5 Y22.25  \n", "T2 M6  \n", "~\n"]
    assert entries[0]["rx"] == [[0.1, "G21"], [0.2, "ok"]]
    extract_real_job.align(entries, GCODE, windows)
    home = extract_real_job.home_of(entries, GCODE)
    assert home == {"x": 1.5, "y": 2.25}
    extract_real_job.align(entries, GCODE, windows, home)
    assert [entry["line"] for entry in entries] == [1, 2, 3, None]


def test_status_reports_are_thinned_to_the_first_and_last_two_with_their_pe_lines():
    rx = [[0.1, "<a>"], [0.2, "[PE:a]"], [0.3, "<b>"], [0.4, "[PE:b]"], [0.5, "<c>"], [0.6, "[PE:c]"],
          [0.7, "<d>"], [0.8, "[PE:d]"], [0.9, "<e>"], [1.0, "[PE:e]"], [1.1, "ok"]]
    kept = [line for _, line in extract_real_job.thin(rx)]
    assert kept == ["<a>", "[PE:a]", "<b>", "[PE:b]", "<d>", "[PE:d]", "<e>", "[PE:e]", "ok"]


def test_z_is_shifted_on_every_line_but_comments():
    assert extract_real_job.shift_z("G1 X1 Z-4.167 F333.", 25.0) == "G1 X1 Z20.833 F333."
    assert extract_real_job.shift_z("G0 Z15", 25.0) == "G0 Z40"
    assert extract_real_job.shift_z("(ZMIN=-21.078)", 25.0) == "(ZMIN=-21.078)"
