"""Turns a serial recording (~/.WebControl/serial-recordings/*.jsonl) into a contract/serial transcript skeleton.

    .venv/bin/python tools/convert_recording.py 20261114-093000.jsonl --gcode part.nc --name cut_part_one
    .venv/bin/python tools/convert_recording.py 20261114-093000.jsonl 20261114-093000.1.jsonl --stdout

The skeleton replays the recorded rx and checks the recorded tx; read it against spec/firmware.md, add expect events
and a description, and trim it to the behaviour it is for.
"""
import argparse
import json
import re
import shutil
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "tests"))

import serial_contract  # noqa: E402

MIN_TICK = 0.25
# The backend drops a connection that says nothing for 5 s, so a longer gap cannot have had no rx in it.
MAX_TICK = 4.5
# A burst of writes after this long without one, and with no action recorded since, needs its cause filled in.
UNEXPLAINED_GAP = 1.0
HANDSHAKE_SECONDS = 3.0
FIRMWARE_VERSION = re.compile(r"^Firmware Version\s+([0-9]+\.[0-9]+)")
KEPT_SECTIONS = ("Maslow Settings", "Advanced Settings")


def read_records(paths):
    return [json.loads(line) for path in paths for line in Path(path).read_text().splitlines() if line.strip()]


def default_settings():
    defaults = json.loads((REPO / "defaultwebcontrol.json").read_text())
    return {section: {option["key"]: option["value"] for option in defaults.get(section, [])} for section in KEPT_SECTIONS}


def changed_settings(recorded):
    '''
    Only the settings that differ from defaultwebcontrol.json, which is what a fresh install starts from.
    '''
    defaults = default_settings()
    changed = {section: {key: value for key, value in (recorded or {}).get(section, {}).items()
                         if key in defaults[section] and str(value) != str(defaults[section][key])}
               for section in KEPT_SECTIONS}
    return {section: values for section, values in changed.items() if values}


def is_status(line):
    return line.startswith("<") or line.startswith("[PE:")


def strip_line_end(data):
    return data[:-2] if data.endswith("\r\n") else data.rstrip("\r\n")


def firmware_of(records):
    version = next((match.group(1) for record in records if record["type"] == "rx"
                    for match in [FIRMWARE_VERSION.match(record["data"])] if match), "51.27")
    rewrites = any(serial_contract.EEPROM_REWRITE_NOTICE in record.get("data", "") for record in records)
    variant = "stock" if float(version) < 50 else "optical" if float(version) >= 100 else "holey"
    return {"variant": variant, "version": version, **({"build": "51.292"} if version == "51.29" and rewrites else {})}


def setup_of(session, gcode_path):
    setup = {"start": "boot", "units": session.get("units") or "MM"}
    home = session.get("home")
    if home:
        setup["home"] = {"x": home["x"], "y": home["y"]}
    settings = changed_settings(session.get("settings"))
    if settings:
        setup["settings"] = settings
    if gcode_path is not None:
        setup["gcode"] = {"path": "gcode/" + Path(gcode_path).name}
    return setup


def events_of(records, drop_status=False, warn=print):
    '''
    The transcript events for a session's records, with ticks for the gaps between them.
    '''
    events = []
    last_t = 0.0
    last_tx_t = None
    action_since_tx = False
    pending_comment = None
    for record in records:
        kind = record["type"]
        if kind in ("session", "gcode"):
            if kind == "gcode" and events:
                warn("a gcode file was loaded at t=%.2f; the transcript keeps only the first one" % record["t"])
            continue
        if kind == "dropped":
            pending_comment = "the recorder dropped %d records here" % record["count"]
            warn(pending_comment + " (t=%.2f)" % record["t"])
            continue
        if kind == "rx" and drop_status and is_status(record["data"]):
            continue
        gap = record["t"] - last_t
        if gap >= MIN_TICK:
            events.append({"type": "tick", "seconds": round(min(gap, MAX_TICK), 2)})
        last_t = record["t"]
        if kind == "ui":
            event = {"type": "ui", "action": {"command": record["command"], "arg": record.get("arg", ""),
                                               "arg1": record.get("arg1", "")}}
            action_since_tx = True
        elif kind == "rx":
            event = {"type": "rx", "line": strip_line_end(record["data"])}
        elif kind == "tx":
            event = serial_contract.tx_event(record["data"])
            quiet = last_tx_t is None or record["t"] - last_tx_t >= UNEXPLAINED_GAP
            if quiet and not action_since_tx and record["t"] > HANDSHAKE_SECONDS:
                event["comment"] = "TODO: no recorded action explains this write; add the ui event that caused it"
            last_tx_t = record["t"]
            action_since_tx = False
        else:
            warn("skipping unknown record type " + kind)
            continue
        if pending_comment is not None:
            event["comment"] = pending_comment + ("; " + event["comment"] if "comment" in event else "")
            pending_comment = None
        events.append(event)
    return events


def convert(records, name, source, gcode_path=None, drop_status=False, warn=print):
    session = next((record for record in records if record["type"] == "session"), {})
    return {
        "formatVersion": serial_contract.FORMAT_VERSION,
        "meta": {
            "name": name,
            "description": "TODO: what this pins down. Converted from " + source + ".",
            "firmware": firmware_of(records),
            "webcontrolCommit": "WebControl " + str(session.get("webcontrol", "unknown")),
        },
        "setup": setup_of(session, gcode_path),
        "events": events_of(records, drop_status, warn),
    }


def transcript_name(text):
    return re.sub(r"[^a-z0-9_]+", "_", text.lower()).strip("_") or "recording"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("recordings", nargs="+", help="the session's .jsonl files, in order")
    parser.add_argument("--gcode", help="the G-code file the session ran; copied into contract/serial/gcode/")
    parser.add_argument("--name", help="transcript name (default: from the first file name)")
    parser.add_argument("--drop-status", action="store_true", help="leave out status and [PE:] reports")
    parser.add_argument("--stdout", action="store_true", help="print instead of writing a file")
    parser.add_argument("--out", default=str(serial_contract.CONTRACT / "transcripts"))
    args = parser.parse_args(argv)
    name = transcript_name(args.name or "recording_" + Path(args.recordings[0]).name.split(".")[0])
    warn = lambda message: print("warning: " + message, file=sys.stderr)  # noqa: E731
    transcript = convert(read_records(args.recordings), name, ", ".join(Path(p).name for p in args.recordings),
                         args.gcode, args.drop_status, warn)
    text = serial_contract.dumps(transcript)
    if args.stdout:
        sys.stdout.write(text)
        return
    if args.gcode:
        target = serial_contract.CONTRACT / "gcode" / Path(args.gcode).name
        if not target.exists():
            shutil.copyfile(args.gcode, target)
    path = Path(args.out) / (name + ".json")
    path.write_text(text)
    print("wrote", path)


if __name__ == "__main__":
    main()
