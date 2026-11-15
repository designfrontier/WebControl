"""tools/import_v094_log.py splits a v0.94 log into one recording per connect (synthetic log in tests/fixtures/v094)."""
import inspect
import itertools
import json
import sys
import time
import tracemalloc

import jsonschema
import pytest

import serial_contract

sys.path.insert(0, str(serial_contract.REPO / "tools"))
import convert_recording  # noqa: E402
import import_v094_log  # noqa: E402

FIXTURES = serial_contract.REPO / "tests" / "fixtures" / "v094"
LOG = FIXTURES / "log.txt"
ALOG = FIXTURES / "alog.txt"
RECORDING_SCHEMA = serial_contract.load(serial_contract.CONTRACT / "recording.schema.json")
TRANSCRIPT_SCHEMA = serial_contract.load(serial_contract.SCHEMA)
HEADER = b"+++++\n2024-03-02 09:14:55\n+++++\n"
CONNECT = b"0.40: Grbl v1.00\n0.50: Sent: B05  \n0.55: ok\n"


def grouped(records):
    starts = [index for index, record in enumerate(records) if record["type"] == "session"]
    return [(import_v094_log.stamp_of(records[begin]), records[begin:end])
            for begin, end in zip(starts, starts[1:] + [len(records)])]


def sessions(log=LOG, alog=None, expected_warnings=(), **options):
    warnings = []
    found = grouped(list(import_v094_log.recordings(log, alog, warnings.append, **options)))
    assert warnings == list(expected_warnings)
    return found


def logged(tmp_path, data, name="log.txt"):
    path = tmp_path / name
    path.write_bytes(data)
    return path


def tx(records):
    return [record["data"] for record in records if record["type"] == "tx"]


def rx(records):
    return [record["data"] for record in records if record["type"] == "rx"]


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

    first, second, _ = [records for _, records in sessions(alog=ALOG)]

    assert tx(first)[-3:] == ["G90  \n", "G90  \n", "G10 Z0  \n"]
    assert rx(second).count("PCB v1.6 TLE9201  Detected\r\n") == 1
    assert [record["t"] for record in first] == sorted(record["t"] for record in first)


def test_alog_starts_are_paired_in_order_even_within_the_same_second(tmp_path):
    suspended = b"0.60: Logging suspended due to user idle time > 20 seconds\n"
    log = logged(tmp_path, HEADER + CONNECT + suspended + HEADER + CONNECT + suspended)
    alog = logged(tmp_path, b"".join(HEADER + CONNECT + b"0.70: Sent: G0 X%d  \n" % n for n in (1, 2)), "alog.txt")

    found = sessions(log, alog)

    assert [tx(records)[-1] for _, records in found] == ["G0 X1  \n", "G0 X2  \n"]


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
    log = logged(tmp_path, LOG.read_bytes().replace(b"0.42: FAKE_SERVO off", b"garbage without a time"))

    found = sessions(log, expected_warnings=["%s:5: not a log entry: 'garbage without a time'" % log])

    assert len(found) == 3
    assert json.dumps(found)


@pytest.mark.parametrize("cut_off", [b"0.60: Sent: G0 X1", b"\x00" * 40])
def test_a_start_glued_onto_a_line_a_crash_cut_off_is_still_a_new_start(tmp_path, cut_off):
    log = logged(tmp_path, HEADER + CONNECT + cut_off + b"+++++\n2024-03-03 10:00:00\n+++++\n" + CONNECT)
    warning = "%s:7: a crash cut this line off before the next start: %r" % (log, (cut_off + b"+++++").decode()[:80])

    found = sessions(log, expected_warnings=[warning])

    assert [stamp for stamp, _ in found] == ["20240302-091455", "20240303-100000"]
    assert [tx(records) for _, records in found] == [["B05  \n"], ["B05  \n"]]


def test_a_last_line_a_crash_cut_off_is_left_out(tmp_path):
    log = logged(tmp_path, HEADER + CONNECT + b"0.60: Sent: G0 X1")

    warning = "%s:7: the last line is cut off and is left out: b'0.60: Sent: G0 X1'" % log

    found = sessions(log, expected_warnings=[warning])

    assert tx(found[0][1]) == ["B05  \n"]


def test_only_a_newline_ends_an_entry(tmp_path):
    log = logged(tmp_path, HEADER + CONNECT + "0.70: noise\rmore\x0cff\x1cfs café\n".encode())

    found = sessions(log)

    assert rx(found[0][1])[-1] == "noise\rmore\x0cff\x1cfs café\r\n"


def test_time_going_back_shifts_what_follows_with_a_warning(tmp_path):
    log = logged(tmp_path, HEADER + CONNECT + b"5.00: Sent: G1 X1  \n4.00: ok\n4.50: ok\n")

    warning = "log.txt:8: time goes back 1.00 s; later entries are shifted forward by that"

    found = sessions(log, expected_warnings=[warning])

    assert [record["t"] for record in found[0][1][1:]] == [0.0, 0.1, 0.15, 4.6, 4.6, 5.1]


def test_webcontrol_messages_logged_as_controller_lines_are_left_out(tmp_path):
    log = logged(tmp_path, HEADER + CONNECT + b"0.80: Message: The bit diameter value of 30.0 mm seems wrong.\n\n"
                 b"Please check the number and enter it again.\n0.90: Message: Cannot open import file.\n0.95: ok\n")

    found = sessions(log)

    assert rx(found[0][1]) == ["Grbl v1.00\r\n", "ok\r\n", "ok\r\n"]


def test_a_long_connect_is_split_at_an_idle_gap_and_at_a_record_limit(tmp_path):
    jobs = b"".join(b"%d.00: Sent: G0 X%d  \n%d.05: ok\n" % (start + n, n, start + n)
                    for start in (10, 4000) for n in range(3))
    log = logged(tmp_path, HEADER + CONNECT + jobs)

    by_idle = sessions(log)
    by_count = sessions(log, max_records=4, split_idle=1e9)

    assert [records[0]["source"] for _, records in by_idle] == ["log.txt line 1, connect 1",
                                                               "log.txt line 1, connect 1, part 2"]
    assert tx(by_idle[1][1]) == ["G0 X0  \n", "G0 X1  \n", "G0 X2  \n"] and by_idle[1][1][1]["t"] == 0
    assert by_idle[1][1][0]["units"] == by_idle[0][1][0]["units"]
    assert [len(records) - 1 for _, records in by_count] == [4, 4, 4, 3]


def synthetic_log(path, connects, lines_per_connect):
    t = itertools.count()
    with open(path, "w") as file:
        file.write(HEADER.decode())
        for _ in range(connects):
            file.write("%.2f: Grbl v1.00\n%.2f: Sent: B05  \n" % (next(t) / 20, next(t) / 20))
            file.writelines("%.2f: Sent: G1 X%d  \n%.2f: <Idle,MPos:%d.00,0.00,0.00>\n"
                            % (next(t) / 20, n, next(t) / 20, n) for n in range(lines_per_connect // 2))
    return path


def test_recordings_stream_in_bounded_memory(tmp_path):
    log = synthetic_log(tmp_path / "log.txt", 2, 80000)
    assert log.stat().st_size > 3000000
    assert inspect.isgenerator(import_v094_log.recordings(log))

    tracemalloc.start()
    import_v094_log.main([str(log), "--out", str(tmp_path / "out")])
    peak = tracemalloc.get_traced_memory()[1]
    tracemalloc.stop()

    assert peak < 1000000
    assert len(list((tmp_path / "out").glob("*.jsonl"))) == 2


def test_finding_connects_takes_linear_time(tmp_path):
    log = synthetic_log(tmp_path / "log.txt", 3000, 40)

    started = time.monotonic()
    count = sum(record["type"] == "session" for record in import_v094_log.recordings(log, warn=pytest.fail))

    assert count == 3000
    assert time.monotonic() - started < 5
