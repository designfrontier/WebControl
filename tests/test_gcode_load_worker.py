"""Gcode loads build off the gevent hub, hand results back on it, run one at a time and always end the spinner."""
import gzip
import json
import threading
import time

import gevent
import pytest

import File.gcodeFile as gcode_file
from DataStructures.data import GCODE_LOADING_MESSAGE, GCODE_LOCKED_MESSAGE
from File.gcodeFile import GCodeFile, Line
from helpers import decode_ui_message, drain


@pytest.fixture
def loader(data):
    gcodeFile = data.gcodeFile
    saved = (data.gcode, data.gcodeIndex, gcodeFile.filename, data.compressedGCode3D, gcodeFile.line3D,
             data.uploadFlag)
    [drain(queue) for queue in (data.console_queue, data.ui_queue1)]
    yield gcodeFile
    (data.gcode, data.gcodeIndex, gcodeFile.filename, data.compressedGCode3D, gcodeFile.line3D,
     data.uploadFlag) = saved
    [drain(queue) for queue in (data.console_queue, data.ui_queue1)]


def ui_messages(data):
    return [decode_ui_message(raw) for raw in drain(data.ui_queue1)]


def test_the_preview_is_built_on_another_thread(data, loader, monkeypatch):
    threads = []
    build = GCodeFile._buildPreview
    monkeypatch.setattr(GCodeFile, "_buildPreview", lambda self: threads.append(threading.get_ident()) or build(self))

    assert loader.loadUpdateFile("G0 X1\nG1 X2 Z-1\n") is True

    assert threads and threads[0] != threading.get_ident()
    assert json.loads(gzip.decompress(data.compressedGCode3D))[-1]["points"][-1][2] < 0


def test_a_load_announces_itself_before_it_builds_and_needs_no_resend(data, loader, monkeypatch):
    seen = []
    build = GCodeFile._buildPreview
    monkeypatch.setattr(GCodeFile, "_buildPreview", lambda self: seen.append(ui_messages(data)) or build(self))

    loader.loadUpdateFile("G0 X1\n")

    assert ("Action", "gcodeLoadStarted", "") in seen[0]
    assert ("Action", "gcodeUpdate", "") not in ui_messages(data)


def test_the_worker_messages_reach_the_real_queues_once_it_finishes(data, loader):
    loader.loadUpdateFile("G0 X1\nG1 X2 Z`3\n")

    console = drain(data.console_queue)
    assert any("Z`3" in line for line in console)
    assert any(line.startswith("compressed3D:") for line in console)


def test_a_worker_exception_is_logged_alerted_and_ends_the_spinner(data, loader, monkeypatch):
    data.gcode = ["G0 X9 "]
    previous = data.compressedGCode3D

    def explode(self):
        raise ValueError("preview broke")

    monkeypatch.setattr(GCodeFile, "_buildPreview", explode)

    assert loader.loadUpdateFile("G0 X1\n") is False

    console = "\n".join(drain(data.console_queue))
    assert "preview broke" in console and "Traceback" in console
    messages = ui_messages(data)
    assert ("Alert", "Alert", "Cannot open gcode file.") in messages
    assert messages[-1] == ("Action", "gcodeUpdate", "")
    assert data.gcode == ["G0 X9 "]
    assert data.compressedGCode3D is previous


def test_a_missing_file_fails_the_same_way(data, loader, tmp_path):
    loader.filename = str(tmp_path / "gone.nc")

    assert loader.loadUpdateFile() is False

    assert loader.filename == ""
    messages = ui_messages(data)
    assert ("Alert", "Alert", "Cannot open gcode file.") in messages
    assert messages[-1] == ("Action", "gcodeUpdate", "")


def test_a_job_that_starts_during_the_build_keeps_its_gcode(data, loader, monkeypatch):
    data.gcode = ["G0 X9 "]
    build = GCodeFile._buildPreview

    def startJobMidBuild(self):
        data.uploadFlag = 1
        build(self)

    monkeypatch.setattr(GCodeFile, "_buildPreview", startJobMidBuild)

    assert loader.loadUpdateFile("G0 X1\n") is False

    assert data.gcode == ["G0 X9 "]
    assert ("Alert", "Alert", GCODE_LOCKED_MESSAGE) in ui_messages(data)


def slow_builds(monkeypatch, log):
    build = GCodeFile._buildPreview

    def slow(self):
        log.append(("start", self.data.gcode[0]))
        time.sleep(0.2)
        build(self)
        log.append(("end", self.data.gcode[0]))

    monkeypatch.setattr(GCodeFile, "_buildPreview", slow)


def test_a_second_load_waits_for_the_first_and_wins(data, loader, monkeypatch):
    log = []
    slow_builds(monkeypatch, log)

    first = gevent.spawn(loader.loadUpdateFile, "G0 X1\n")
    gevent.sleep(0.05)
    assert loader.isLoading()
    second = gevent.spawn(loader.loadUpdateFile, "G0 X2\n")
    gevent.joinall([first, second])

    assert [first.value, second.value] == [True, True]
    assert log == [("start", "G0 X1 "), ("end", "G0 X1 "), ("start", "G0 X2 "), ("end", "G0 X2 ")]
    assert data.gcode[0] == "G0 X2 "
    assert not loader.isLoading()


def test_a_failed_load_leaves_the_next_loads_file_named(data, loader, monkeypatch, tmp_path):
    log = []
    slow_builds(monkeypatch, log)
    good = tmp_path / "good.nc"
    good.write_text("G0 X3\n")
    loader.filename = str(tmp_path / "gone.nc")

    first = gevent.spawn(loader.loadUpdateFile)
    gevent.sleep(0)
    loader.filename = str(good)
    second = gevent.spawn(loader.loadUpdateFile)
    gevent.joinall([first, second])

    assert [first.value, second.value] == [False, True]
    assert loader.filename == str(good)
    assert data.gcode[0] == "G0 X3 "


def test_play_is_refused_while_a_load_is_building(data, loader, monkeypatch):
    log = []
    slow_builds(monkeypatch, log)
    data.gcode = ["G0 X9 "]

    load = gevent.spawn(loader.loadUpdateFile, "G0 X1\n")
    gevent.sleep(0.05)
    drain(data.ui_queue1)
    data.actions.processAction({"data": {"command": "startRun", "arg": "", "arg1": ""}})
    refused = ui_messages(data)
    load.join()

    assert ("Alert", "Alert", GCODE_LOADING_MESSAGE) in refused
    assert data.uploadFlag == 0


@pytest.mark.parametrize("count", [0, 1, 499, 500, 501, 1234])
def test_the_chunked_payload_matches_one_json_dump(count, monkeypatch):
    monkeypatch.setattr(gcode_file, "PREVIEW_CHUNK_MOVES", 500)
    items = [{"type": "line", "points": [[n, n / 3, -n]], "dashed": bool(n % 2), "command": None} for n in range(count)]

    length, compressed = gcode_file._gzipJsonList(items)

    assert gzip.decompress(compressed).decode() == json.dumps(items)
    assert length == len(json.dumps(items))


def test_chunked_tidying_matches_tidying_the_whole_text(monkeypatch, data, loader):
    text = "G1X1.234567Y2.5  Z-0.1\n\n(comment)\nG0  X3\n" * 7
    monkeypatch.setattr(gcode_file, "PARSE_CHUNK_LINES", 3)
    chunked = loader._workerCopy()
    chunked._parse(text, "3")
    monkeypatch.setattr(gcode_file, "PARSE_CHUNK_LINES", 10_000)
    whole = loader._workerCopy()
    whole._parse(text, "3")

    assert chunked.data.gcode == whole.data.gcode
    assert "G1 X1.234 Y2.5 Z-0.1 " in whole.data.gcode


def test_board_cells_are_marked_on_another_thread(data, monkeypatch):
    threads = []
    mark = type(data.boardManager)._markCutCells
    monkeypatch.setattr(type(data.boardManager), "_markCutCells",
                        lambda self, *args: threads.append(threading.get_ident()) or mark(self, *args))
    board = data.boardManager.getCurrentBoard()
    saved = list(board.cutPoints)
    line = Line()
    line.type, line.points = "circle", [(0, 0, -1), (1, 0, -1)]
    monkeypatch.setattr(data.gcodeFile, "line3D", [line])

    assert data.boardManager.processGCode() is True

    board.cutPoints = saved
    assert threads and threads[0] != threading.get_ident()


def test_a_board_worker_exception_is_logged_and_reported(data, monkeypatch):
    def explode(self, *args):
        raise ValueError("cells broke")

    monkeypatch.setattr(type(data.boardManager), "_markCutCells", explode)
    drain(data.console_queue)
    drain(data.ui_queue1)

    data.actions.processAction({"data": {"command": "boardProcessGCode", "arg": "", "arg1": ""}})

    assert "cells broke" in "\n".join(drain(data.console_queue))
    assert ("Alert", "Alert", "Error with processing gcode") in ui_messages(data)
