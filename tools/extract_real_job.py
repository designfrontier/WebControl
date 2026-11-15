"""Cuts a real job out of the owner's recordings: a trimmed G-code file and what v0.94 sent and read for its lines.

    .venv/bin/python tools/extract_real_job.py --recordings ~/webcontrol-backup/recordings \
        --gcode ~/webcontrol-backup/WebControl-20261114/gcode [name ...]
    .venv/bin/python tools/extract_real_job.py ... --probe RECORDING T0 T1 FILE [START_LINE]

Writes contract/serial/gcode/real/<name>.nc (the kept line ranges of the file, back to back) and
contract/serial/real-jobs/<name>.json: v0.94's writes in the job's time windows, each with the file line it came
from and the controller lines up to the next write, status reports thinned. It streams the recording.
"""
import argparse
import json
import re
import statistics
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CONTRACT = REPO / "contract" / "serial"
WORD = re.compile(r"([A-Z])\s*([-+]?(?:[0-9]+\.?[0-9]*|\.[0-9]+))")
Z_WORD = re.compile(r"Z\s*([-+]?(?:[0-9]+\.?[0-9]*|\.[0-9]+))")
LOOKAHEAD = 400
REPORTS_KEPT = 2

# Each window is [t0, t1, the file line streaming starts at]; segments are the file's [start, end) lines kept.
JOBS = {
    "real_mancala_outside_tool_change": {
        "recording": "20260629-143512.jsonl", "file": "mancala/mancala.frd.outside.25.endmill.nc",
        "windows": [[344.0, 616.25, 0], [716.3, 815.0, 113]], "segments": [[0, 74], [113, 137]],
    },
    "real_mancala_roughing_tool_change": {
        "recording": "20261114-160443.jsonl", "file": "mancala/mancala.frd.roughing.25.endmill.nc",
        "windows": [[2834.0, 2876.0, 0]], "segments": [[0, 150]],
    },
    "real_mancala_finishing_large_job": {
        "recording": "20261114-233640.jsonl", "file": "mancala/mancala.finishing.best.25.ballnose.nc",
        "windows": [[725.0, 975.0, 0], [13990.0, 14082.0, 120000]], "segments": [[0, 150], [120375, 120415]],
    },
    "real_katie_neck_back_pause_jog_resume": {
        "recording": "20251215-123916.jsonl", "file": "katie-guitar/1-neck-back-25-endmill.nc",
        "windows": [[1123.5, 1170.0, 0], [4400.0, 4685.0, 8800]], "segments": [[0, 40], [8975, 9040]],
        # Every Z this job streamed is 25 mm above today's file, which was regenerated after the cut.
        "zShift": 25.0, "home": {"x": 6.2, "y": 34.0},
    },
    "real_katie_body_top_pause_stop": {
        "recording": "20251216-182101.jsonl", "file": "katie-guitar/1-body-top-25-endmill.nc",
        "windows": [[489.0, 572.75, 0], [755.0, 790.0, 1150]], "segments": [[0, 120], [1190, 1245]],
    },
    "real_maypole_tool_change_play": {
        "recording": "20260428-052325.jsonl", "file": "maypole/maypole-125-endmill.nc",
        "windows": [[1692.0, 1737.0, 0]], "segments": [[0, 150]],
    },
}


def strip_comments(line):
    return re.sub(r"\(.*?\)", "", line).split(";")[0].strip().upper()


def words(line):
    return [(letter, float(value)) for letter, value in WORD.findall(strip_comments(line))]


def shape(line):
    '''
    A line's words with X and Y values left out, which v0.94 shifted by home.
    '''
    return tuple((letter, None if letter in "XY" else value) for letter, value in words(line))


def shift_z(line, dz):
    number = lambda match: "Z" + ("%.4f" % (float(match.group(1)) + dz)).rstrip("0").rstrip(".")  # noqa: E731
    return line if line.lstrip().startswith("(") else Z_WORD.sub(number, line)


def read_windows(path, windows):
    '''
    v0.94's writes inside the windows, each with the controller lines read after it until the next write.
    '''
    entries = []
    last = max(t1 for _, t1, _ in windows)
    with open(path) as handle:
        for raw in handle:
            if raw.startswith('{"type": "session'):
                continue
            at = raw.index('"t": ') + 5
            t = float(raw[at:raw.index(",", at)])
            inside = next((n for n, (t0, t1, _) in enumerate(windows) if t0 <= t <= t1), None)
            if t > last:
                break
            if inside is None:
                continue
            record = json.loads(raw)
            if record["type"] == "tx":
                entries.append({"t": t, "window": inside, "tx": record["data"], "rx": []})
            elif record["type"] == "rx" and entries and entries[-1]["window"] == inside:
                entries[-1]["rx"].append([round(t - entries[-1]["t"], 2), record["data"].rstrip("\r\n")])
    return entries


def shifted(line, home):
    '''
    A line's words as v0.94 sent them: X and Y moved by home, every value to 3 decimals.
    '''
    offset = {"X": home["x"], "Y": home["y"]} if home else {}
    return tuple((letter, round(value + offset.get(letter, 0.0), 3) if home else
                  (None if letter in "XY" else value)) for letter, value in words(line))


def align(entries, lines, windows, home=None):
    '''
    Gives each write the file line it streams: the next line with the same words, moving forward through the file.
    Without a home, X and Y are left out of the comparison.
    '''
    keys = [shifted(line, home) if strip_comments(line) else None for line in lines]
    cursor, window = 0, None
    for entry in entries:
        if entry["window"] != window:
            window, cursor = entry["window"], windows[entry["window"]][2]
        key = shifted(entry["tx"], {"x": 0.0, "y": 0.0}) if home else shape(entry["tx"])
        hit = next((n for n in range(cursor, min(cursor + LOOKAHEAD, len(lines))) if keys[n] == key and key), None)
        entry["line"] = hit
        cursor = hit + 1 if hit is not None else cursor


def home_of(entries, lines):
    offsets = {"X": [], "Y": []}
    for entry in entries:
        if entry["line"] is None:
            continue
        sent, filed = dict(words(entry["tx"])), dict(words(lines[entry["line"]]))
        [offsets[axis].append(sent[axis] - filed[axis]) for axis in "XY" if axis in sent and axis in filed]
    return {axis.lower(): round(statistics.median(values), 4) if values else 0.0 for axis, values in offsets.items()}


def thin(rx):
    '''
    Keeps every controller line but the status reports, of which the first and last REPORTS_KEPT pairs stay.
    '''
    reports = [n for n, (_, line) in enumerate(rx) if line.startswith("<")]
    kept = set(reports[:REPORTS_KEPT] + reports[-REPORTS_KEPT:])
    dropped = lambda n, line: ((line.startswith("<") and n not in kept)  # noqa: E731
                               or (line.startswith("[PE:") and n > 0 and rx[n - 1][1].startswith("<")
                                   and n - 1 not in kept))
    return [item for n, item in enumerate(rx) if not dropped(n, item[1])]


def load_lines(gcode_root, job):
    lines = (gcode_root / job["file"]).read_text(errors="replace").splitlines()
    dz = job.get("zShift")
    return [shift_z(line, dz) for line in lines] if dz else lines


def extract(name, job, recordings, gcode_root):
    lines = load_lines(gcode_root, job)
    entries = read_windows(recordings / job["recording"], job["windows"])
    align(entries, lines, job["windows"])
    home = job.get("home") or home_of(entries, lines)
    align(entries, lines, job["windows"], home)
    kept = [n for start, end in job["segments"] for n in range(start, end)]
    position = {n: index for index, n in enumerate(kept)}
    for entry in entries:
        entry["trimmedLine"] = position.get(entry["line"])
        entry["rx"] = thin(entry["rx"])
        del entry["window"]
    meta = {key: job[key] for key in ("recording", "file", "windows", "segments", "zShift") if key in job}
    excerpt = {"name": name, **meta, "fileLines": len(lines), "home": home}
    (CONTRACT / "gcode" / "real").mkdir(parents=True, exist_ok=True)
    (CONTRACT / "real-jobs").mkdir(parents=True, exist_ok=True)
    (CONTRACT / "gcode" / "real" / (name + ".nc")).write_text("\n".join(lines[n] for n in kept) + "\n")
    head = json.dumps(excerpt, indent=2)[:-2]
    body = ",\n".join("    " + json.dumps(entry) for entry in entries)
    (CONTRACT / "real-jobs" / (name + ".json")).write_text(head + ',\n  "entries": [\n' + body + "\n  ]\n}\n")
    matched = sum(entry["trimmedLine"] is not None for entry in entries)
    print(name, "home", excerpt["home"], "writes", len(entries), "on kept lines", matched, "kept", len(kept))


def probe(recording, t0, t1, lines, start):
    windows = [[t0, t1, start]]
    entries = read_windows(recording, windows)
    align(entries, lines, windows)
    home = home_of(entries, lines)
    align(entries, lines, windows, home)
    print("home", home)
    for entry in entries:
        others = [line for _, line in entry["rx"]
                  if not line.startswith(("<", "[PE:", "ok")) and line != entry["tx"].strip()]
        took = next((dt for dt, line in entry["rx"] if line == "ok"), None)
        print("%9.2f %6s %5s %-50s %s" % (entry["t"], entry["line"], took, entry["tx"].rstrip()[:50], others[:4]))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("names", nargs="*")
    parser.add_argument("--recordings", type=Path, required=True)
    parser.add_argument("--gcode", type=Path, required=True)
    parser.add_argument("--probe", nargs="+", metavar="ARG", help="RECORDING T0 T1 FILE [START_LINE]")
    args = parser.parse_args(argv)
    if args.probe:
        recording, t0, t1, path, *start = args.probe
        lines = load_lines(args.gcode, {"file": path})
        return probe(args.recordings / recording, float(t0), float(t1), lines, int(start[0]) if start else 0)
    unknown = [name for name in args.names if name not in JOBS]
    if unknown:
        parser.error("unknown job(s): " + ", ".join(unknown))
    [extract(name, JOBS[name], args.recordings, args.gcode) for name in args.names or sorted(JOBS)]


if __name__ == "__main__":
    sys.exit(main())
