"""Scenarios for real jobs from the owner's machine (contract/serial/real-jobs/, made by tools/extract_real_job.py).

Each streams a trimmed file of the owner's with the owner's settings and home, against a controller that answers
as the owner's 51.27 did: its echo, its tool-change lines and the status reports it sent while each line ran.
"""
import json
import re

import owner_machine
from serial_scenarios import FINISHED, expect, firmware, idle, pauses_on, scenario, tick, ui, wait_tx

import serial_contract

REAL_JOBS = serial_contract.CONTRACT / "real-jobs"
WORD = re.compile(r"([A-Z])\s*([-+]?(?:[0-9]+\.?[0-9]*|\.[0-9]+))")
# Lines with no number but the G or M word (G21, G90, M5) recur, so they match only the writes right ahead.
NEAR, FAR = 3, 60
MIN_TICK, MAX_TICK = 0.25, 1.0


def line_key(line):
    return tuple((letter, round(float(value), 3)) for letter, value in WORD.findall(line.upper()))


def excerpt(name):
    return json.loads((REAL_JOBS / (name + ".json")).read_text())


class RecordedReports:
    '''
    For each line the backend sends, the status reports v0.94's matching write got before its ok, with ticks no
    longer than MAX_TICK for the time between them. A line v0.94 never sent gets none.
    '''

    def __init__(self, entries):
        self.entries = entries
        self.cursor = 0

    def __call__(self, line):
        key = line_key(line)
        reach = FAR if any(letter in "XYZIJF" for letter, _ in key) else NEAR
        end = min(self.cursor + reach, len(self.entries))
        hit = next((n for n in range(self.cursor, end) if line_key(self.entries[n]["tx"]) == key), None)
        if hit is None:
            return []
        self.cursor = hit + 1
        return while_running(self.entries[hit]["rx"])


def while_running(rx):
    ok = next((dt for dt, line in rx if line == "ok"), None)
    reports = [(dt, line) for dt, line in rx if line.startswith(("<", "[PE:")) and (ok is None or dt < ok)]
    out, last = [], 0.0
    for dt, line in reports:
        out += [tick(round(min(dt - last, MAX_TICK), 2))] if dt - last >= MIN_TICK else []
        out.append(line)
        last = dt
    return out + ([tick(round(min(ok - last, MAX_TICK), 2))] if ok is not None and ok - last >= MIN_TICK else [])


def tool_change(tool):
    return pauses_on(r"\bM6\b", "Tool Change: Please insert tool %d" % tool, "Maslow Paused")


def real_job(name, description, steps):
    job = excerpt(name)
    setup = {"units": "MM", "home": job["home"], "settings": owner_machine.SETTINGS,
             "gcode": {"path": "gcode/real/" + name + ".nc"}}
    scenario(name, description + " From the owner's " + job["file"] + " (" + job["recording"] + ").", setup,
             [firmware(reports=RecordedReports(job["entries"])), *steps])


def tool_change_then(tool, resume):
    return [tool_change(tool), ui("startRun"), idle(),
            expect(state={"uploadFlag": 0, "jobInProgress": True, "manualZAxisAdjust": True, "currentTool": tool}),
            ui(resume)]


real_job(
    "real_mancala_outside_tool_change",
    "A small contour with arcs, its first pass and its last back to back: T2 M6 stops streaming and the firmware "
    "asks for tool 2; Resume returns to the file's XY at safe height and the job runs to M30.",
    [*tool_change_then(2, "resumeRun"), idle(), expect(state=FINISHED)],
)

real_job(
    "real_mancala_roughing_tool_change",
    "The first 150 lines of a 45,000-line adaptive roughing job: tool change, Resume, then the ramp in at 333 mm/min.",
    [*tool_change_then(1, "resumeRun"), idle(), expect(state=FINISHED)],
)

real_job(
    "real_mancala_finishing_large_job",
    "A 120,000-line finishing job, its first 150 lines and its last 40 (arcs, the retract, M5, M30) back to back.",
    [*tool_change_then(3, "resumeRun"), idle(), expect(state=FINISHED)],
)

real_job(
    "real_katie_neck_back_pause_jog_resume",
    "The only Pause and Resume in the owner's logs: Pause during a 33 s move, Z jogged down 11 mm and up in "
    "steps, Define Z zero, Z up 10, Resume. The file's Z is 25 mm up on today's copy, as it was cut.",
    [*tool_change_then(1, "resumeRun"), wait_tx(r"X227\.526 Y63\.836"),
     ui("pauseRun", comment="the 33 s move to X227.526 Y63.836 is still running"), idle(),
     expect(state={"uploadFlag": 2, "jobInProgress": True, "pauseButtonSetting": "Resume"}),
     *[step for direction, distance in [("lower", "10"), ("lower", "1"), ("raise", "0.25"), ("raise", "0.25"),
                                        ("raise", "0.25"), ("lower", "0.125")]
       for step in (ui("moveZ", direction, distance), idle())],
     ui("defineZ0"), idle(), ui("moveZ", "raise", "10"), idle(),
     ui("resumeRun"), idle(), expect(state=FINISHED)],
)

real_job(
    "real_katie_body_top_pause_stop",
    "Tool change answered with Play, then helical arcs; Pause during the 11 s retract, then Stop: ! goes out, the "
    "firmware reloads its position, and the job ends.",
    [*tool_change_then(1, "startRun"), wait_tx(r"^G1 Z15 "),
     ui("pauseRun", comment="the retract is still running"), idle(), ui("stopRun"), idle(),
     expect(state=FINISHED)],
)

real_job(
    "real_maypole_tool_change_play",
    "Tool change answered with Play (a resume), then the first 150 lines of a 44,500-line job.",
    [*tool_change_then(5, "startRun"), idle(), expect(state=FINISHED)],
)
