"""Summarises serial recordings (tools/import_v094_log.py output) one JSON line per file, streaming each file.

    .venv/bin/python tools/index_recordings.py ~/webcontrol-backup/recordings/*.jsonl > index.jsonl

Each summary has the record counts, the first and last `t`, and for every feature below its count and the `t` of
its first occurrence, so a session with a tool change or an alarm can be found without loading it.
"""
import argparse
import json
import re
import sys
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

# Controller lines, matched as substrings of the JSON-encoded record.
RX_FEATURES = {
    "bootBanner": re.compile(r'"data": "(PCB v1\.|Grbl v1\.00|ready\\r)'),
    "settingsDump": re.compile(r'"data": "\$0='),
    "firmwareVersion": re.compile(r'"data": "Firmware Version'),
    "toolChange": re.compile(r"Tool Change:"),
    "maslowPaused": re.compile(r"Maslow Paused"),
    "alarm": re.compile(r"ALARM:"),
    "error": re.compile(r'"data": "error:'),
    "message": re.compile(r'"data": "Message:'),
    "zAxisZeroed": re.compile(r"z axis zeroed"),
    "bufferOverflow": re.compile(r"Buffer overflow!"),
    "unsupported": re.compile(r"unsupported and ignored"),
    "status": re.compile(r'"data": "<'),
    "positionError": re.compile(r'"data": "\[PE:'),
    "ok": re.compile(r'"data": "ok\\r\\n"'),
}
# What WebControl wrote, matched against the decoded line.
TX_FEATURES = {
    "B05": re.compile(r"^B05\b"),
    "settingsQuery": re.compile(r"^\$\$"),
    "settingWrite": re.compile(r"^\$\d+="),
    "calibration": re.compile(r"^B(0[2468]|1[0-5])\b"),
    "jog": re.compile(r"^G91\b"),
    "toolChangeM6": re.compile(r"\bM0?6\b"),
    "pauseM0": re.compile(r"\bM0?[01]\b(?!\.)"),
    "resume": re.compile(r"^~$"),
    "stop": re.compile(r"^!$"),
    "arc": re.compile(r"\bG0?[23]\b"),
    "probe": re.compile(r"G38\.2"),
    "zeroZ": re.compile(r"^G10 Z0"),
    "inches": re.compile(r"^G20\b"),
    "millimetres": re.compile(r"^G21\b"),
    "cutMove": re.compile(r"^G0?1 .*[XY]"),
}


def tail_t(path):
    '''
    The `t` of the last record, read from the end of the file.
    '''
    with open(path, "rb") as handle:
        handle.seek(0, 2)
        handle.seek(max(0, handle.tell() - 65536))
        lines = [line for line in handle.read().splitlines() if line.strip()]
    return json.loads(lines[-1])["t"] if lines else None


def summarise(path):
    summary = {"file": Path(path).name, "tx": 0, "rx": 0, "first": None, "last": tail_t(path), "features": {}}
    features = summary["features"]

    def seen(name, t):
        entry = features.setdefault(name, {"count": 0, "firstT": t})
        entry["count"] += 1

    with open(path, encoding="utf-8", errors="replace") as handle:
        for raw in handle:
            if raw.startswith('{"type": "session"'):
                session = json.loads(raw)
                summary.update(started=session.get("started"), source=session.get("source"))
                continue
            kind = raw[10:12]
            if kind not in ("tx", "rx"):
                continue
            t = float(raw[raw.index('"t": ') + 5:raw.index(",", raw.index('"t": '))])
            summary["first"] = t if summary["first"] is None else summary["first"]
            summary[kind] += 1
            if kind == "rx":
                [seen(name, t) for name, pattern in RX_FEATURES.items() if pattern.search(raw)]
            else:
                data = json.loads(raw)["data"].rstrip("\n").strip()
                [seen(name, t) for name, pattern in TX_FEATURES.items() if pattern.search(data)]
    return summary


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("recordings", nargs="+")
    parser.add_argument("--jobs", type=int, default=8)
    args = parser.parse_args(argv)
    paths = sorted(path for path in args.recordings if path.endswith(".jsonl"))
    with ProcessPoolExecutor(args.jobs) as pool:
        [print(json.dumps(summary), flush=True) for summary in pool.map(summarise, paths)]


if __name__ == "__main__":
    main()
