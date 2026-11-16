"""Runs Python's optical pipeline (git 220aa0e) on the Rust-rendered fixture frames and writes python_golden.json.

Run through golden.sh, which first regenerates the frames. Needs opencv-python 4.6.0.66 and numpy 1.23.5.
"""

import contextlib
import io
import json
import subprocess
import sys
import tempfile
import time
import types
from pathlib import Path

import cv2
import numpy as np

COMMIT = "220aa0e"
REPO = Path(__file__).resolve().parents[2]
FIXTURES = REPO / "rust/crates/wc-optical/tests/fixtures"
FRAMES_PER_MEASUREMENT = 10


def load_module(scratch):
    """Imports Actions/opticalCalibration.py from the commit, with makesmithInitFuncs stubbed and no sleeping."""
    source = subprocess.run(
        ["git", "show", f"{COMMIT}:Actions/opticalCalibration.py"], cwd=REPO, check=True, capture_output=True, text=True
    ).stdout
    for package, files in {
        "Actions": {"opticalCalibration.py": source},
        "DataStructures": {"makesmithInitFuncs.py": "class MakesmithInitFuncs:\n    pass\n"},
    }.items():
        (scratch / package).mkdir()
        (scratch / package / "__init__.py").write_text("")
        for name, text in files.items():
            (scratch / package / name).write_text(text)
    sys.path.insert(0, str(scratch))
    time.sleep = lambda seconds: None
    import Actions.opticalCalibration as module

    return module


def opencv3_shim():
    """cv2 with minAreaRect's angle mapped into OpenCV 3's [-90, 0), the range Python's angle logic assumes."""
    real = cv2.minAreaRect

    def legacy(points):
        (cx, cy), (w, h), angle = real(points)
        return ((cx, cy), (h, w), angle - 90) if angle > 0 else ((cx, cy), (w, h), angle)

    shim = types.SimpleNamespace(**{name: getattr(cv2, name) for name in dir(cv2) if not name.startswith("__")})
    shim.minAreaRect = legacy
    return shim


class Quiet:
    def put(self, *args):
        pass


class FakeCamera:
    def __init__(self, frame):
        self.frame = frame
        self.reads = 0

    def readNewFrame(self, previous, timeout=5.0):
        self.reads += 1
        return self.frame, self.reads


def rounded(value):
    return None if value is None else round(float(value), 4)


def measure(module, optical, frame, find_centre, opencv):
    module.cv2 = opencv
    optical.camera = FakeCamera(frame)
    with contextlib.redirect_stdout(io.StringIO()):
        dx, dy, dist, xb, yb, xa, ya, _ = optical.processImage(findCenter=find_centre)
    if dx is None:
        return None
    return {
        "dxMm": rounded(dx),
        "dyMm": rounded(dy),
        "distMm": rounded(dist),
        "centrePx": [rounded(xb), rounded(yb)],
        "opticalCentrePx": [rounded(xa), rounded(ya)],
    }


def frame_cases(module, optical):
    manifest = json.loads((FIXTURES / "frames.json").read_text())["cases"]
    shim = opencv3_shim()
    out = []
    for case in manifest:
        gray = cv2.imread(str(FIXTURES / case["png"]), cv2.IMREAD_GRAYSCALE)
        frame = cv2.cvtColor(gray, cv2.COLOR_GRAY2BGR)
        scene = case["scene"]
        optical.markerX = case["target"]["markerMm"][0] / 25.4
        optical.markerY = case["target"]["markerMm"][1] / 25.4
        centre = scene["opticalCentrePx"]
        default = centre == [scene["width"] / 2, scene["height"] / 2]
        optical.opticalCenter = (0, 0) if default else tuple(centre)
        out.append(
            {
                "name": case["name"],
                "truth": case["truth"],
                "findCentre": measure(module, optical, frame, True, cv2),
                "calibrateRaw": measure(module, optical, frame, False, cv2),
                "calibrateShim": measure(module, optical, frame, False, shim),
            }
        )
    return out


def csv_cases(module, optical, scratch):
    """Python's CSV of seeded grids: one full, one with awkward roundings, one sparse (None measured as 0)."""
    rng = np.random.default_rng(220)
    full = np.round(rng.normal(0.0, 1.5, (2, 31, 15)), 4)
    awkward = np.zeros((2, 31, 15))
    awkward.flat[:8] = [-0.001, 0.005, 0.015, 2.675, -0.0, -2.675, 1e-9, -1.005]
    awkward.flat[8:] = np.round(rng.uniform(-30, 30, awkward.size - 8), 3)
    sparse = np.round(rng.normal(0.0, 0.8, (2, 31, 15)), 4)
    mask = rng.uniform(size=(31, 15)) < 0.3
    (scratch / ".WebControl").mkdir()
    optical.data.config = types.SimpleNamespace(getHome=lambda: str(scratch))
    out = []
    for name, grid, measured in [("full", full, None), ("awkward", awkward, None), ("sparse", sparse, mask)]:
        filled = grid if measured is None else np.where(measured, grid, 0.0)
        optical.calErrorsX, optical.calErrorsY = filled[0], filled[1]
        results = scratch / ".WebControl/OpticalCalibrationResults"
        for old in results.glob("*.csv") if results.exists() else []:
            old.unlink()
        with contextlib.redirect_stdout(io.StringIO()):
            assert optical.saveCalibrationToCSV()
        (written,) = results.glob("*.csv")
        wire = [
            [[None if measured is not None and not measured[c][r] else float(axis[c][r]) for r in range(15)] for c in range(31)]
            for axis in grid
        ]
        out.append({"name": name, "errorX": wire[0], "errorY": wire[1], "text": written.read_text()})
    return out


def one_case_a_line(golden):
    """Reviewable in a diff without one number a line."""
    lists = [
        f' "{key}": [\n' + ",\n".join("  " + json.dumps(case) for case in golden[key]) + "\n ]"
        for key in ("frames", "csv")
    ]
    return "{\n" + ",\n".join([' "source": ' + json.dumps(golden["source"])] + lists) + "\n}\n"


def main():
    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp)
        module = load_module(scratch)
        optical = module.OpticalCalibration()
        optical.data = types.SimpleNamespace(console_queue=Quiet())
        golden = {
            "source": {
                "commit": COMMIT,
                "function": "Actions/opticalCalibration.py processImage and saveCalibrationToCSV",
                "opencv": cv2.__version__,
                "numpy": np.__version__,
                "framesPerMeasurement": FRAMES_PER_MEASUREMENT,
                "settings": {"gaussianBlurValue": 5, "cannyLowValue": 50, "cannyHighValue": 100},
            },
            "frames": frame_cases(module, optical),
            "csv": csv_cases(module, optical, scratch),
        }
    path = FIXTURES / "python_golden.json"
    path.write_text(one_case_a_line(golden))
    print(f"wrote {path.relative_to(REPO)}")


if __name__ == "__main__":
    main()
