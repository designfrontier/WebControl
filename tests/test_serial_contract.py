"""Every transcript in contract/serial/transcripts replays against the real backend, and the runner is strict."""
import copy
import json

import jsonschema
import pytest

import serial_contract
from serial_contract import ContractFailure, load, replay, transcripts

SCHEMA = load(serial_contract.SCHEMA)
TRANSCRIPTS = transcripts()


def test_the_schema_is_valid_json_schema():
    jsonschema.Draft202012Validator.check_schema(SCHEMA)


def test_there_is_a_corpus():
    assert len(TRANSCRIPTS) >= 30


@pytest.mark.parametrize("path", TRANSCRIPTS, ids=[path.stem for path in TRANSCRIPTS])
def test_transcript_replays_against_the_backend(app, path):
    transcript = load(path)
    jsonschema.validate(transcript, SCHEMA)
    assert transcript["meta"]["name"] == path.stem
    assert path.read_text() == serial_contract.dumps(transcript), "not in the recorder's layout"

    replay(app, transcript)


JOG = {
    "formatVersion": 1,
    "meta": {"name": "jog", "description": "jog", "firmware": {"variant": "holey", "version": "51.27"},
             "webcontrolCommit": "test"},
    "setup": {"units": "MM", "home": {"x": 0, "y": 0}},
    "events": [
        {"type": "ui", "action": {"command": "move", "arg": "right", "arg1": "10"}},
        {"type": "tx", "bytes": "G91  \n"},
        {"type": "rx", "line": "ok"},
        {"type": "tx", "bytes": "G00 X10.0  \n"},
        {"type": "rx", "line": "ok"},
        {"type": "tx", "bytes": "G90  \n"},
        {"type": "rx", "line": "ok"},
    ],
}


def jog(edit):
    transcript = copy.deepcopy(JOG)
    edit(transcript["events"])
    jsonschema.validate(transcript, SCHEMA)
    return transcript


def test_the_jog_transcript_passes(app):
    replay(app, JOG)


def test_a_different_byte_fails(app):
    def edit(events):
        events[3]["bytes"] = "G00 X10.0 \n"

    with pytest.raises(ContractFailure, match="but the backend wrote"):
        replay(app, jog(edit))


def test_a_write_the_transcript_does_not_list_fails(app):
    with pytest.raises(ContractFailure, match="unexpected tx"):
        replay(app, jog(lambda events: events.pop(3)))


def test_a_trailing_write_the_transcript_does_not_list_fails(app):
    with pytest.raises(ContractFailure, match="unexpected tx"):
        replay(app, jog(lambda events: events.__delitem__(slice(5, None))))


def test_a_write_that_never_comes_fails(app):
    def edit(events):
        events.append({"type": "tx", "bytes": "G0 X0 \n"})

    with pytest.raises(ContractFailure, match="never wrote"):
        replay(app, jog(edit))


def test_expecting_a_line_before_the_ok_that_releases_it_fails(app):
    def edit(events):
        events[2], events[3] = events[3], events[2]

    with pytest.raises(ContractFailure, match="never wrote"):
        replay(app, jog(edit))


def test_a_realtime_write_must_be_marked(app):
    def edit(events):
        events.extend([{"type": "ui", "action": {"command": "stopRun", "arg": "", "arg1": ""}},
                       {"type": "tx", "bytes": "!\n"}])

    with pytest.raises(ContractFailure, match="realtime"):
        replay(app, jog(edit))


def test_a_wrong_state_expectation_fails(app):
    def edit(events):
        events.append({"type": "expect", "state": {"positioningMode": "G91"}})

    with pytest.raises(ContractFailure, match="positioningMode"):
        replay(app, jog(edit))


def test_an_alert_that_was_not_emitted_fails(app):
    def edit(events):
        events.append({"type": "expect", "alerts": [{"title": "Line Too Long"}]})

    with pytest.raises(ContractFailure, match="nothing emitted matches"):
        replay(app, jog(edit))


def test_a_replay_leaves_the_backend_as_it_found_it(app, data):
    before = (data.units, data.gcode, data.uploadFlag, json.dumps(data.config.settings, sort_keys=True))

    replay(app, JOG)

    assert (data.units, data.gcode, data.uploadFlag, json.dumps(data.config.settings, sort_keys=True)) == before


def test_substring_matching_reaches_into_nested_fields():
    assert serial_contract.matches({"data": {"title": "Too"}}, {"data": {"title": "Line Too Long", "x": 1}})
    assert not serial_contract.matches({"data": {"title": "Short"}}, {"data": {"title": "Line Too Long"}})
    assert not serial_contract.matches({"data": {"n": 1}}, {"data": {"n": "1"}})


def test_a_line_split_across_writes_is_one_tx(app, monkeypatch):
    write = serial_contract.FakePort.write
    monkeypatch.setattr(serial_contract.FakePort, "write",
                        lambda port, payload: [write(port, part) for part in (payload[:2], payload[2:])])

    replay(app, JOG)


def test_two_lines_in_one_write_are_two_txs():
    port = serial_contract.FakePort(lambda port: None, serial_contract.Clock())

    port.write(b"G91  \nG00 X10.0  \nG9")

    assert port.written == ["G91  \n", "G00 X10.0  \n"] and port.partial == "G9"


def test_a_write_left_without_its_newline_fails(app, monkeypatch):
    write = serial_contract.FakePort.write
    monkeypatch.setattr(serial_contract.FakePort, "write",
                        lambda port, payload: write(port, payload.replace(b"G90  \n", b"G90  ")))

    with pytest.raises(ContractFailure, match="never wrote|without a newline"):
        replay(app, JOG)


def test_a_write_later_than_the_tx_timeout_fails(app, monkeypatch):
    write = serial_contract.FakePort.write

    def late(port, payload):
        port.clock.sleep(2.5 if payload.startswith(b"G00") else 0)
        write(port, payload)

    monkeypatch.setattr(serial_contract.FakePort, "write", late)

    with pytest.raises(ContractFailure, match="after the 2.0 s tx timeout"):
        replay(app, JOG)


def test_values_compare_as_serde_json_would():
    assert serial_contract.same(254, 254.0)
    assert not serial_contract.same(True, 1)
    assert not serial_contract.same(0, False)
    assert not serial_contract.matches({"data": {"on": 1}}, {"data": {"on": True}})


def test_an_expect_that_checks_nothing_is_not_a_valid_transcript():
    with pytest.raises(jsonschema.ValidationError):
        jog(lambda events: events.append({"type": "expect", "comment": "nothing"}))
    with pytest.raises(jsonschema.ValidationError):
        jog(lambda events: events.append({"type": "expect", "alerts": [{}]}))
    with pytest.raises(jsonschema.ValidationError):
        jog(lambda events: events.append({"type": "tx", "bytes": "G0 X1\nG0 X2\n"}))
