"""Loads a large gcode file under real gevent patching and measures how long the hub stalls meanwhile.

Run as a script: `python tests/gcode_load_probe.py <lines> <result.json>`. A ticker greenlet records every wake-up, a
Stop is queued shortly after the load begins, and a stand-in serial loop writes it to a fake port.
"""
import atexit
import json
import os
import shutil
import sys
import tempfile
import time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
os.environ["HOME"] = tempfile.mkdtemp(prefix="webcontrol-probe-")
atexit.register(shutil.rmtree, os.environ["HOME"], ignore_errors=True)
os.chdir(REPO)
sys.path.insert(0, REPO)

from gevent import monkey  # noqa: E402

monkey.patch_all()

import gevent  # noqa: E402

from Background.webcamVideoStream import WebcamVideoStream  # noqa: E402
from Connection.serialPort import SerialPort  # noqa: E402
from Connection.serialPortThread import SerialPortThread  # noqa: E402
from DataStructures.data import Data  # noqa: E402

SerialPort.openConnection = lambda self, *args: None
WebcamVideoStream.getSettings = lambda self, src=0: None
Data.lastChecked = float("inf")

import schedule  # noqa: E402

import main  # noqa: E402

schedule.clear()

STOP_AFTER = 0.3
TICK = 0.01


class FakePort:
    is_open = True
    in_waiting = 0

    def __init__(self):
        self.writes = []

    def readline(self):
        return b""

    def write(self, payload):
        self.writes.append((time.perf_counter(), payload.decode()))


def write_gcode(path, count):
    def block(n):
        x, y = n % 997 / 10, n % 499 / 10
        plunge = ["G0 Z5", "G0 X%.3f Y%.3f" % (x, y), "G1 Z-1 F300"]
        return plunge + ["G1 X%.3f Y%.3f F1200" % (x + step / 10, y + step / 20) for step in range(1, 48)]

    lines = ["G21", "G90"] + [line for n in range(count // 50 + 1) for line in block(n)]
    with open(path, "w") as out:
        out.write("\n".join(lines[:count]))


def probe(lines):
    data = main.app.data
    path = os.path.join(os.environ["HOME"], "large.nc")
    write_gcode(path, lines)

    port = FakePort()
    serial = SerialPortThread()
    serial.data = data
    serial.serialInstance = port
    running = True
    ticks = []

    def serve():
        while running:
            serial._exchangeLines()
            time.sleep(TICK)

    def tick():
        while running:
            ticks.append(time.perf_counter())
            gevent.sleep(TICK)

    workers = [gevent.spawn(serve), gevent.spawn(tick)]
    gevent.sleep(0.1)

    data.gcodeFile.filename = path
    data.compressedGCode3D = None
    started = time.perf_counter()
    loader = gevent.spawn(data.gcodeFile.loadUpdateFile)
    gevent.spawn_later(STOP_AFTER, data.actions.processAction, {"data": {"command": "stopRun", "arg": "", "arg1": ""}})
    loader.join()
    while data.compressedGCode3D is None:
        gevent.sleep(0.005)
    finished = time.perf_counter()

    deadline = finished + 2
    while not any(text.startswith("!") for _, text in port.writes) and time.perf_counter() < deadline:
        gevent.sleep(TICK)
    running = False
    gevent.joinall(workers)

    during = [t for t in ticks if started <= t <= finished]
    edges = [started] + during + [finished]
    stops = [when for when, text in port.writes if text.startswith("!")]
    return {
        "loaded": loader.value,
        "lines": len(data.gcode),
        "parseSeconds": finished - started,
        "maxGapSeconds": max(b - a for a, b in zip(edges, edges[1:])),
        "stopLatencySeconds": stops[0] - (started + STOP_AFTER) if stops else None,
    }


if __name__ == "__main__":
    result = probe(int(sys.argv[1]))
    with open(sys.argv[2], "w") as out:
        json.dump(result, out)
