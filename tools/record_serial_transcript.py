"""Runs scenarios from tools/serial_scenarios.py against the real backend and writes contract/serial transcripts.

    .venv/bin/python tools/record_serial_transcript.py            # every scenario
    .venv/bin/python tools/record_serial_transcript.py jog_each_direction_mm --stdout
"""
import argparse
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "tests"))
sys.path.insert(0, str(REPO / "tools"))

import conftest  # noqa: E402  boots main.py's app on a throwaway HOME, as the tests do
import serial_contract  # noqa: E402
from serial_scenarios import SCENARIOS  # noqa: E402


def commit():
    try:
        return subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=REPO, capture_output=True, text=True,
                              check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def steps_to_events(engine, steps):
    for step in steps:
        kind = step["type"]
        if kind == "idle":
            yield from engine.idle()
        elif kind == "waitTx":
            yield from engine.wait_tx(step["pattern"])
        elif kind == "firmware":
            apply_firmware_step(engine.firmware, step, engine.port.written)
        else:
            yield step


def apply_firmware_step(model, step, written):
    if step.get("flush") or step.get("forget"):
        model.flush()
    if step.get("forget"):
        model.seen = len(written)
    if step.get("ack") is not None:
        model.ack = step["ack"]
    if step.get("echo") is not None:
        model.echo = step["echo"]
    if step.get("reports") is not None:
        model.reports = step["reports"]
    rule = step.get("rule")
    if rule:
        model.add_rule(rule["pattern"], rule["replies"], rule.get("hold", False))


def record(app, spec, webcontrol_commit):
    with serial_contract.Engine(app, spec["setup"], spec.get("timing"), "record", spec["firmware"]) as engine:
        events = engine.run(steps_to_events(engine, spec["steps"]))
    transcript = {
        "formatVersion": serial_contract.FORMAT_VERSION,
        "meta": {
            "name": spec["name"],
            "description": spec["description"],
            "firmware": spec["firmware"],
            "webcontrolCommit": webcontrol_commit,
        },
        "setup": spec["setup"],
    }
    if "timing" in spec:
        transcript["timing"] = spec["timing"]
    transcript["events"] = events
    return transcript


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("names", nargs="*", help="scenario names (default: all)")
    parser.add_argument("--stdout", action="store_true", help="print instead of writing files")
    parser.add_argument("--out", default=str(serial_contract.CONTRACT / "transcripts"))
    args = parser.parse_args(argv)
    names = args.names or sorted(SCENARIOS)
    unknown = [name for name in names if name not in SCENARIOS]
    if unknown:
        parser.error("unknown scenario(s): " + ", ".join(unknown))
    webcontrol_commit = commit()
    failed = []
    for name in names:
        try:
            text = serial_contract.dumps(record(conftest.main.app, SCENARIOS[name], webcontrol_commit))
        except serial_contract.ContractFailure as failure:
            failed.append(name)
            print("FAILED", name + ":", failure, file=sys.stderr)
            continue
        if args.stdout:
            sys.stdout.write(text)
        else:
            path = Path(args.out) / (name + ".json")
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
            print("wrote", path.relative_to(REPO) if path.is_relative_to(REPO) else path)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
