"""tools/import_v094_log.py splits a v0.94 log into one recording per connect (synthetic log in tests/fixtures/v094)."""
import json
import sys

import jsonschema

import serial_contract

sys.path.insert(0, str(serial_contract.REPO / "tools"))
import convert_recording  # noqa: E402
import import_v094_log  # noqa: E402

FIXTURES = serial_contract.REPO / "tests" / "fixtures" / "v094"
LOG = FIXTURES / "log.txt"
ALOG = FIXTURES / "alog.txt"
RECORDING_SCHEMA = serial_contract.load(serial_contract.CONTRACT / "recording.schema.json")
TRANSCRIPT_SCHEMA = serial_contract.load(serial_contract.SCHEMA)


def sessions(alog=None):
    warnings = []
    found = import_v094_log.sessions(LOG, alog, warnings.append)
    assert warnings == []
    return found


def tx(records):
    return [record["data"] for record in records if record["type"] == "tx"]


def test_each_connect_in_each_webcontrol_start_is_a_session():
    found = sessions()

    assert [stamp for stamp, _ in found] == ["20240302-091455", "20240302-091630", "20240303-180210"]
    assert [records[0]["source"] for _, records in found] == [
        "log.txt line 1, connect 1", "log.txt line 1, connect 2", "log.txt line 58, connect 1",
    ]


def test_a_session_starts_at_the_boot_banner_and_keeps_writes_byte_for_byte():
    _, records = sessions()[0]

    assert records[0]["started"] == "2024-03-02T09:14:55.410000"
    assert records[1] == {"type": "rx", "t": 0.0, "data": "PCB v1.6 TLE9201  Detected\r\n"}
    assert tx(records)[:4] == ["B05  \n", "G21  \n", "$$ \n", "$3=463 \n"]
    assert not any("Ground Control Version" in record.get("data", "") for record in records)
    assert any(record.get("data", "").startswith("[PE:") for record in records)
    [jsonschema.validate(record, RECORDING_SCHEMA) for record in records]


def test_the_handshake_gives_the_units_and_the_settings_webcontrol_ended_up_with():
    first, _, third = [records[0] for _, records in sessions()]

    assert first["units"] == "MM" and third["units"] == "INCHES"
    assert first["settings"] == {
        "Maslow Settings": {"bedWidth": "2438.4", "motorSpacingX": "2978.4", "motorOffsetY": "463"},
        "Advanced Settings": {"chainElasticity": "0.00000517"},
    }


def test_alog_fills_what_log_txt_skipped_while_logging_was_suspended():
    assert "G10 Z0  \n" not in tx(sessions()[0][1])

    records = sessions(ALOG)[0][1]

    assert "G10 Z0  \n" in tx(records)
    assert [record["t"] for record in records] == sorted(record["t"] for record in records)


def test_the_cli_writes_recordings_the_converter_turns_into_valid_transcripts(tmp_path, capsys):
    import_v094_log.main([str(LOG), "--alog", str(ALOG), "--out", str(tmp_path)])

    paths = sorted(tmp_path.glob("*.jsonl"))
    assert [path.name for path in paths] == ["20240302-091455.jsonl", "20240302-091630.jsonl", "20240303-180210.jsonl"]
    assert "8 tx" in capsys.readouterr().out
    records = convert_recording.read_records([paths[1]])
    transcript = convert_recording.convert(records, "v094_job", paths[1].name, warn=lambda message: None)
    jsonschema.validate(transcript, TRANSCRIPT_SCHEMA)
    assert transcript["setup"]["start"] == "boot"
    assert transcript["events"][-1] == {"type": "rx", "line": "ok"}
    todo = [event for event in transcript["events"] if "TODO" in event.get("comment", "")]
    assert [(event["bytes"], event.get("realtime", False)) for event in todo] == [("G21  \n", False), ("!\n", True)]


def test_unparseable_lines_are_reported_not_fatal(tmp_path):
    log = tmp_path / "log.txt"
    log.write_text(LOG.read_text().replace("0.42: FAKE_SERVO off", "garbage without a time"))
    warnings = []

    found = import_v094_log.sessions(log, None, warnings.append)

    assert len(found) == 3
    assert warnings == ["%s:5: not a log entry: 'garbage without a time'" % log]
    assert json.dumps(found)
