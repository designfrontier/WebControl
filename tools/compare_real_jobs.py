"""Compares what the backend sends for each real job (its transcript) with what v0.94 sent (contract/serial/real-jobs).

    .venv/bin/python tools/compare_real_jobs.py            # rewrites contract/serial/real-jobs.md
    .venv/bin/python tools/compare_real_jobs.py --stdout

Writes are paired by aligning the two streams on their words (values to 3 decimals). A file line both sent is
identical, an intended difference (a rule below names the finding or commit), or UNEXPLAINED. Writes only one
side made are classified the same way, by the sequence they belong to.
"""
import argparse
import difflib
import json
import re
from collections import Counter
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CONTRACT = REPO / "contract" / "serial"
NUMBER = r"[-+]?(?:[0-9]+\.?[0-9]*|\.[0-9]+)"
WORD = re.compile(r"([A-Z])\s*(%s)" % NUMBER)
XY = re.compile(r"([XY])\s*(%s)" % NUMBER)
REPORT = CONTRACT / "real-jobs.md"

HOME_DECIMALS = "Home-shifted X/Y printed at their own precision, not as float sums (#41, 3a58d71)"
NO_AUTO_RESUME = "No `~` sent by itself when the firmware asks for a tool (A3, d43640b)"
TOOL_RESUME = ("Resume or Play after a tool change returns to the file's XY at safe height or higher (efc9d2c, 0bf6da8, "
               "7fd6ce2); v0.94's Play restarted mid-file with `G0 Z5.0 F12.8`, a rapid to the XY, the M6 again, "
               "`G4` and `G0 Z0`")
PAUSE_RESUME = ("Resume after a Pause returns at safe height or higher, restores the spindle and plunges with G1 at the "
                "file's plunge feed, then restores the file's F (#5, #17 b73f519, A4 1f6c0bd, c1400bd, 877dd8d); "
                "v0.94 sent `G90` and `G0 Z<paused Z>`")

# The lines each intended sequence may hold, by side; anything else in it is unexplained.
SEQUENCES = {
    TOOL_RESUME: {
        "backend": re.compile(r"~|G2[01]|G90|G0 Z{n}|G0 X{n} Y{n}".format(n=NUMBER)),
        "v094": re.compile(r"~|G0 Z{n} F{n}|G0 X{n} Y{n} F{n}|T\d+ M6|G4|G0 Z0".format(n=NUMBER)),
    },
    PAUSE_RESUME: {
        "backend": re.compile(r"~|G2[01]|G90|G0 Z{n}|G0 X{n} Y{n}|M[34]|G1 Z{n} F{n}|G1 F{n}".format(n=NUMBER)),
        "v094": re.compile(r"~|G90|G0 Z{n}".format(n=NUMBER)),
    },
}

# Intended by the rules above, but each is a choice the owner's files make visible.
NOTES = [
    "- Every Fusion file here has its `T<n> M6` before any XY move, so Resume after the tool change returns to the "
    "file's XY there, which is X0 Y0 shifted by home: a rapid to the home point before the file's own first rapid.",
    "- Resume after a Pause restores the spindle with a bare `M3` (the file said `M3 S5000`). On 51.27 with `$17=0` "
    "both do nothing (`setSpindlePower`, H27 `Spindle.cpp`). The one bare `M3` in the logs (v0.94's, "
    "20251221-000533 t=36) was never echoed or acked: the controller sent nothing for 16 s, then rebooted.",
    "- Resume never lowers the bit before the XY move (owner decision, 2026-11-15): after the Pause it travels at the "
    "bit's 10 mm above a Z zero just redefined, and after each tool change at the reported 10.03 mm rounded up a step, "
    "not at the 5 mm safe height; v0.94 went straight to the paused Z.",
]


def words(line):
    return [(letter, float(value)) for letter, value in WORD.findall(line.upper())]


def key(line):
    return tuple((letter, round(value, 3)) for letter, value in words(line))


def tx_lines(transcript):
    return [event["bytes"] for event in transcript["events"] if event["type"] == "tx"]


def xy_rounded(line):
    return XY.sub(lambda match: match.group(1) + repr(round(float(match.group(2)), 6)), line)


def classify_pair(v094, backend):
    return HOME_DECIMALS if v094 != backend and xy_rounded(v094) == xy_rounded(backend) else None


def auto_resume(entries, n):
    '''
    v0.94's own `~`, written within half a second of the firmware's `Maslow Paused`.
    '''
    previous = entries[n - 1] if n > 0 else None
    return (entries[n]["tx"].strip() == "~" and previous is not None
            and any(line == "Maslow Paused" for _, line in previous["rx"])
            and entries[n]["t"] - previous["t"] < 0.5)


def classify_only(side, lines, context):
    '''
    Writes one side made and the other did not: the category they belong to, or None.
    '''
    category = (TOOL_RESUME if context.get("resumeAfterToolChange") else
                PAUSE_RESUME if context.get("resumeAfterPause") else None)
    return category if category and all(SEQUENCES[category][side].fullmatch(line.strip()) for line in lines) else None


def compare(name):
    excerpt = json.loads((CONTRACT / "real-jobs" / (name + ".json")).read_text())
    backend = tx_lines(json.loads((CONTRACT / "transcripts" / (name + ".json")).read_text()))
    # Streamed lines outside the kept segments are not in the backend's file.
    kept = [entry for entry in excerpt["entries"] if entry["line"] is None or entry["trimmedLine"] is not None]
    entries = [entry for n, entry in enumerate(kept) if not auto_resume(kept, n)]
    v094 = [entry["tx"] for entry in entries]
    matcher = difflib.SequenceMatcher(None, [key(line) for line in v094], [key(line) for line in backend],
                                      autojunk=False)
    result = {"name": name, "file": excerpt["file"], "recording": excerpt["recording"], "compared": 0,
              "identical": 0, "intended": Counter(), "unexplained": [], "sequences": []}
    result["intended"][NO_AUTO_RESUME] += len(kept) - len(entries)
    for op, a0, a1, b0, b1 in matcher.get_opcodes():
        pairs = list(zip(range(a0, a1), range(b0, b1))) if op == "equal" or a1 - a0 == b1 - b0 else []
        for a, b in pairs:
            file_line = entries[a]["trimmedLine"]
            result["compared"] += file_line is not None
            category = classify_pair(v094[a], backend[b])
            if v094[a] == backend[b]:
                result["identical"] += file_line is not None
            elif category:
                result["intended"][category] += 1
            else:
                result["unexplained"].append(("line %s" % (file_line + 1 if file_line is not None else "-"),
                                              v094[a], backend[b]))
        if pairs:
            continue
        context = context_of(entries, v094, backend, a0, b0)
        for side, lines in (("v094", v094[a0:a1]), ("backend", backend[b0:b1])):
            if not lines:
                continue
            files = [entries[a]["trimmedLine"] for a in range(a0, a1)] if side == "v094" else []
            result["compared"] += sum(line is not None for line in files)
            # A file line only v0.94 sent is never a sequence difference.
            category = None if any(line is not None for line in files) else classify_only(side, lines, context)
            if category:
                result["intended"][category] += len(lines)
                result["sequences"].append((side, category, "".join(lines)))
            else:
                result["unexplained"].append(("only " + side, "".join(lines), ""))
    return result


def context_of(entries, v094, backend, a0, b0):
    '''
    What a one-sided block follows: the tool change prompt itself, the Resume (or Play) after a tool change, or the
    Resume after a Pause, judged by the last file line v0.94 streamed before it and the writes since.
    '''
    before = [line for entry in entries[max(0, a0 - 1):a0] for _, line in entry["rx"]]
    last_file = max((n for n in range(a0) if entries[n]["trimmedLine"] is not None), default=None)
    since = [line.strip() for line in v094[(last_file + 1 if last_file is not None else 0):a0]]
    since += [line.strip() for line in backend[max(0, b0 - 12):b0]]
    after_m6 = last_file is not None and re.search(r"\bM0?6\b", v094[last_file]) is not None
    resumed = "~" in since
    return {"afterToolChange": any(line.startswith("Tool Change:") or line == "Maslow Paused" for line in before),
            "resumeAfterToolChange": after_m6,
            "resumeAfterPause": resumed and not after_m6}


def report(results):
    out = ["# Real jobs: the backend against v0.94", "",
           "Generated by `tools/compare_real_jobs.py` from the `real_*` transcripts and `real-jobs/*.json` (what "
           "v0.94 sent on the owner's machine, cut from its logs by `tools/extract_real_job.py`). Writes are paired "
           "by their words; a file line compared is one v0.94 streamed in the job's kept segments. Intended "
           "differences cite the finding or commit; anything else is UNEXPLAINED and listed in full.", "",
           "| Job | File | Lines compared | Identical | Intended differences | Unexplained |",
           "| --- | --- | --- | --- | --- | --- |"]
    for r in results:
        intended = "<br>".join("%d × %s" % (count, category.split(" (")[0]) for category, count in
                               sorted(r["intended"].items())) or "-"
        out.append("| `%s` | `%s` | %d | %d | %s | %d |" % (r["name"], r["file"], r["compared"], r["identical"],
                                                            intended, len(r["unexplained"])))
    categories = sorted({category for r in results for category in r["intended"]})
    out += ["", "## Intended differences", ""] + ["- " + category for category in categories]
    out += ["", "Writes only one side made, as classified:", ""]
    out += ["- `%s` %s only, %s: `%s`" % (r["name"], "v0.94" if side == "v094" else "backend", category.split(" (")[0],
                                         lines.replace("\n", "⏎"))
            for r in results for side, category, lines in r["sequences"]]
    out += ["", "## Worth a look", "", *NOTES]
    out += ["", "## UNEXPLAINED differences", ""]
    unexplained = [(r["name"], item) for r in results for item in r["unexplained"]]
    if not unexplained:
        out.append("None.")
    for name, (where, v094, backend) in unexplained:
        out.append("- `%s` %s: v0.94 `%s`, backend `%s`" % (name, where, v094.replace("\n", "⏎"),
                                                             backend.replace("\n", "⏎")))
    return "\n".join(out) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--stdout", action="store_true")
    args = parser.parse_args(argv)
    names = sorted(path.stem for path in (CONTRACT / "real-jobs").glob("*.json"))
    text = report([compare(name) for name in names])
    if args.stdout:
        print(text, end="")
    else:
        REPORT.write_text(text)
        print("wrote", REPORT.relative_to(REPO))


if __name__ == "__main__":
    main()
