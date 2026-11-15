"""Builds the gcode and board golden fixtures from the Python code; see tests/fixtures/gcode/README.md."""
import base64
import gzip
import json
import os

from Boards.boards import Board
from Connection import firmwareGcode
from Connection.serialPortThread import SerialPortThread
from helpers import drain

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FILES = os.path.join(ROOT, "tests", "fixtures", "gcode", "files")

SHIFTS = [
    {"x": 0.0, "y": 0.0, "truncate": -1},
    {"x": 41.6763, "y": 60.1447, "truncate": -1},
    {"x": -12.3456, "y": 7.89, "truncate": 4},
    {"x": 100.0, "y": -0.1, "truncate": 2},
    {"x": 1e-05, "y": 25.4, "truncate": -1},
]
HOMES = [{"x": 0.0, "y": 0.0}, {"x": 12.5, "y": -7.25}]
TOOL_STATES = [(0, 0), (2, 0), (2, 2), (3, 1)]
EXTRA_LINES = [
    "", " ", "T2 ", "T 3 ", "N10 T2 ", "T2.0 ", "T2 M6 ", "T2.0 M6 ", "T 2 M6 ", "t3 g0 x1 ", "M6 T2 ",
    "N10 T2 M6 ", "G0 X1 T2 ", "B14 T2 M6 ", "B14 L1 S0 F1 I1 T2 V1 ", "$$", "  T2 M6", "T2.5 M6 ", "M6T2 ",
    "T3M6 ", "M06 ", "M006 ", "M60 ", "M6.1 ", "M0 ", "M00 ", "M01 ", "M001. ", "M0.5 ", "M10 ", "M1", "m0 ",
    "G1 X1 M0", "M2 M1 ", "M6" + " " * 121, "M6" + " " * 123, "G1 X1 (unclosed", "G1 X1 ! ~ ` ? ",
    "\tG1\x01X1\x7f ", "G90 G91 X1 ", "G91 G90 X1 ", "G91.1 X1 ", "G90G91X1", "G1 X" + "1" * 121,
    "G1 X" + "1" * 122, "G1 X1 (ok) Y2 ; note", "(only a comment)", "; only a comment", "G1 x5 y5 ",
    "G1 X 10.5 Y -2", "G1 X-0 Y-0.0", "G1 X12345678901234567890.5 ", "G1 X0.1234567 Y-0.0000004 ",
]


def _lines_of(name, data, truncate=None):
    config = data.config
    config.setValue("Advanced Settings", "truncate", 0 if truncate is None else 1, recursionBreaker=True)
    config.setValue("Advanced Settings", "digits", str(truncate or 4), recursionBreaker=True)
    data.gcodeFile.filename = os.path.join(FILES, name)
    data.tolerance = 0.5
    data.gcodeFile.absoluteFlag = 0
    data.gcodeFile.currentDrawTool = 0
    assert data.gcodeFile.loadUpdateFile() is True
    return list(data.gcode)


def _moves(data):
    moves = [vars(line) for line in data.gcodeFile.line3D]
    assert all(move["color"] is None and move["radius"] is None for move in moves)
    return [{key: move[key] for key in ("type", "points", "dashed", "command")} for move in moves]


def _sampled(count):
    return list(range(count + 1)) if count <= 60 else sorted(set(range(0, count + 1, 97)) | {count})


def load_case(data, name):
    case = {"file": name}
    for units in ("MM", "INCHES"):
        data.units = units
        lines = _lines_of(name, data)
        case[units] = {"zMoves": list(data.zMoves), "tolerance": data.tolerance, "fileUnits": data.gcodeFileUnits,
                       "moves": _moves(data)}
    case["lines"] = lines
    case["truncated"] = {str(digits): _lines_of(name, data, digits) for digits in (4, 2)}
    return case


def shift_case(data, lines):
    sample = lines if len(lines) <= 60 else lines[:60] + lines[60::50]
    out = []
    for shift in SHIFTS:
        data.gcodeShift = [shift["x"], shift["y"]]
        data.gcodeFile.truncate = shift["truncate"]
        out.append({**shift, "moved": [[line, data.gcodeFile.moveLine(line)] for line in sample]})
    return out


def _restart(data, index, zTarget, changeTool):
    drain(data.gcode_queue)
    try:
        state = data.actions.gcodeStateAt(index)
    except Exception as error:
        return {"error": type(error).__name__}
    stopped = data.actions.queueRestartSequence(dict(state), zTarget, changeTool)
    return {"state": state, "sent": drain(data.gcode_queue), "stoppedAtToolChange": stopped}


def restart_case(data, lines):
    data.gcode = lines
    cases = []
    for units in ("MM", "INCHES"):
        data.units = units
        for home in HOMES:
            data.config.setValue("Advanced Settings", "homeX", str(home["x"]), recursionBreaker=True)
            data.config.setValue("Advanced Settings", "homeY", str(home["y"]), recursionBreaker=True)
            for currentTool in (0, 2):
                data.currentTool = currentTool
                for index in _sampled(len(lines)):
                    for zTarget, changeTool in ((None, True), (1.5, False)):
                        cases.append({
                            "units": units, "home": home, "currentTool": currentTool, "index": index,
                            "zTarget": zTarget, "changeTool": changeTool,
                            **_restart(data, index, zTarget, changeTool),
                        })
    return cases


def position_case(data, lines):
    data.gcode = lines
    out = []
    for index in _sampled(len(lines)):
        data.feedRate = None
        try:
            position = list(data.actions.findPositionAt(index))
            out.append({"index": index, "position": position, "feedRate": data.feedRate})
        except Exception as error:
            out.append({"index": index, "error": type(error).__name__})
    return out


def gcode_case(data, name):
    data.config.setValue("Maslow Settings", "zAxisSafeHeight", "5", recursionBreaker=True)
    case = load_case(data, name)
    lines = case["lines"]
    case["shift"] = shift_case(data, lines)
    case["restart"] = restart_case(data, lines)
    case["position"] = position_case(data, lines)
    return case


def stream_case(data, lines):
    thread = SerialPortThread()
    thread.data = data
    data.gcodeShift = [41.6763, 60.1447]
    data.gcodeFile.truncate = -1
    out = []
    for raw in lines:
        line = thread._sanitise(raw)
        entry = {
            "raw": raw, "stripped": thread._stripComments(raw), "sanitised": line,
            "positioningMode": thread._positioningModeSetBy(line),
            "pauses": firmwareGcode.pausesFirmware(line), "changesTool": firmwareGcode.changesTool(line),
            "onlySelectsTool": firmwareGcode.onlySelectsTool(line), "toolNamed": firmwareGcode.toolNamedBy(line),
            "toolFirmwareReads": firmwareGcode.toolTheFirmwareReads(line),
            "toolChange": [[nextTool, currentTool, *firmwareGcode.toolChangeLine(line, nextTool, currentTool)]
                           for nextTool, currentTool in TOOL_STATES],
        }
        for mode in (0, 1):
            data.positioningMode = mode
            toSend = firmwareGcode.toolChangeLine(line, 0, 0)[0]
            problem = thread._streamProblem(toSend)
            entry["mode" + str(mode)] = {"shifted": thread._shiftToHome(line),
                                         "problem": None if problem is None else problem[0]}
        out.append(entry)
    return out


def all_stream_lines(cases):
    seen = []
    for line in [line for case in cases for line in case["lines"][:60]] + EXTRA_LINES:
        if line not in seen:
            seen.append(line)
    return seen


BOARDS = [
    {"name": "48x24 centred", "width": 48, "height": 24, "centerX": 0, "centerY": 0},
    {"name": "24x12 offset", "width": 24, "height": 12, "centerX": 12, "centerY": 6},
    {"name": "fractional", "width": 30.5, "height": 15.25, "centerX": 10.25, "centerY": 7.6},
]
CUT_FILES = ["square_mm.nc", "pocket_restart_mm.nc", "inch_arcs.nc", "vcarve_mm.nc", "spiral_large_mm.nc",
             "relative_g91.nc", "fusion_inch.nc"]
CUT_SHIFTS = [{"units": "MM", "shift": [0.0, 0.0]}, {"units": "MM", "shift": [-150.5, -80.25]},
              {"units": "INCHES", "shift": [-2.5, 1.25]}]
TRIMS = [
    {"top": "0", "bottom": "0", "left": "0", "right": "0", "units": "inches"},
    {"top": "2", "bottom": "1", "left": "3", "right": "0.5", "units": "inches"},
    {"top": "25.4", "bottom": "12.7", "left": "50.8", "right": "6.35", "units": "mm"},
    {"top": "1.234", "bottom": "0.0004", "left": "2.9999", "right": "0", "units": "inches"},
]
INFO_EDITS = [
    {"boardID": "oak", "material": "plywood", "height": "24", "width": "48", "thickness": "0.75", "centerX": "0",
     "centerY": "0", "units": "inches"},
    {"boardID": "mdf", "material": "MDF", "height": "609.6", "width": "1219.2", "thickness": "18", "centerX": "-100",
     "centerY": "33.3", "units": "mm"},
    {"boardID": "x", "material": "y", "height": "1.005", "width": "2.675", "thickness": "0.125", "centerX": "0.015",
     "centerY": "-0.025", "units": "inches"},
    {"boardID": "bad", "material": "y", "height": "tall", "width": "2", "thickness": "1", "centerX": "0",
     "centerY": "0", "units": "inches"},
]


def _board_state(board):
    return {"info": json.loads(board.getBoardInfoJSON()), "cutPoints": list(board.getCutPoints())}


def _fresh(board, spec):
    board.updateBoardInfo("-", "-", spec["height"], spec["width"], 0.75, spec["centerX"], spec["centerY"], "inches")
    board.cutPoints = []
    board.boardFilename = ""


def cut_grid_cases(data):
    manager = data.boardManager
    board = manager.getCurrentBoard()
    out = []
    for name in CUT_FILES:
        for setup in CUT_SHIFTS:
            data.units = setup["units"]
            _lines_of(name, data)
            data.gcodeShift = list(setup["shift"])
            for spec in BOARDS:
                _fresh(board, spec)
                manager.processGCode()
                first = list(board.getCutPoints())
                data.gcodeShift = [setup["shift"][0] + 5, setup["shift"][1] - 3]
                manager.processGCode()
                data.gcodeShift = list(setup["shift"])
                out.append({"file": name, **setup, "board": spec, "cut": first, "cutAgainShifted": board.getCutPoints()})
    return out


def trim_cases(data):
    manager = data.boardManager
    board = manager.getCurrentBoard()
    data.units = "MM"
    _lines_of("vcarve_mm.nc", data)
    data.gcodeShift = [-100.0, -50.0]
    out = []
    for spec in BOARDS[:2]:
        for trim in TRIMS:
            _fresh(board, spec)
            manager.processGCode()
            before = list(board.getCutPoints())
            manager.trimBoard({"trimTop": trim["top"], "trimBottom": trim["bottom"], "trimLeft": trim["left"],
                               "trimRight": trim["right"], "units": trim["units"]})
            out.append({"board": spec, "trim": trim, "before": before, "after": _board_state(board)})
    return out


def info_cases(data):
    board = Board()
    out = []
    for edit in INFO_EDITS:
        accepted = board.updateBoardInfo(edit["boardID"], edit["material"], edit["height"], edit["width"],
                                         edit["thickness"], edit["centerX"], edit["centerY"], edit["units"])
        out.append({"edit": edit, "accepted": accepted, "info": json.loads(board.getBoardInfoJSON())})
    bed = []
    for width, height in (("2438.4", "1219.2"), ("3048", "1524"), ("2440.94", "1221.11")):
        data.config.setValue("Maslow Settings", "bedWidth", width, recursionBreaker=True)
        data.config.setValue("Maslow Settings", "bedHeight", height, recursionBreaker=True)
        data.boardManager.initializeNewBoard()
        state = _board_state(data.boardManager.getCurrentBoard())
        bed.append({"bedWidth": width, "bedHeight": height, **state,
                    "cutData": json.loads(gzip.decompress(data.boardManager.getCurrentBoard().getCompressedCutData()))})
    return out, bed


def _board_file(info, cells):
    return json.dumps(info) + "\n" + base64.b64encode(gzip.compress(json.dumps(cells).encode())).decode() + "\n"


def file_cases(data, tmp):
    manager = data.boardManager
    hull = [[1.5, 2.25, 0], [10.0, 2.0, 0], [10.5, 8.75, 0], [1.0, 9.0, 0]]
    info = {"width": 24.0, "height": 12.0, "thickness": 0.75, "centerX": 0.0, "centerY": 0.0, "boardID": "old",
            "material": "pine", "fileName": "old.brd"}
    texts = {
        "legacy hull board": _board_file(info, hull),
        "string dimensions": _board_file({**info, "width": "24", "height": "12.0", "fileName": "s.brd"}, [False] * 4),
        "integer dimensions": _board_file({**info, "width": 4, "height": 1, "fileName": "i.brd"}, [False, True] * 2),
        "info only": json.dumps({**info, "boardID": "üñí", "fileName": "u.brd"}) + "\n",
    }
    out = []
    data.units = "MM"
    _lines_of("square_mm.nc", data)
    data.gcodeShift = [0.0, 0.0]
    for name, text in texts.items():
        path = os.path.join(tmp, "in.brd")
        with open(path, "w") as handle:
            handle.write(text)
        manager.initializeNewBoard()
        loaded = manager.loadBoard(path)
        entry = {"name": name, "text": text, "loaded": loaded, "state": _board_state(manager.getCurrentBoard())}
        if name in ("legacy hull board", "integer dimensions"):
            try:
                manager.processGCode()
                entry["afterProcess"] = manager.getCurrentBoard().getCutPoints()
            except Exception as error:
                entry["afterProcess"] = {"error": type(error).__name__}
        saved = manager.saveBoard("out.brd", tmp)
        with open(os.path.join(tmp, "out.brd")) as handle:
            first, second = handle.read().split("\n")[:2]
        entry["saved"] = {"ok": saved, "infoLine": first, "cutData": json.loads(gzip.decompress(base64.b64decode(second)))}
        out.append(entry)
    return out
