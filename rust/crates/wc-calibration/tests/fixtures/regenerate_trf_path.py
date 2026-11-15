"""Records scipy's iteration path through Python's holey Calibrate for every holey_calibrate.json case.

Run from the repository root with the backend venv (scipy 1.9): .venv/bin/python
rust/crates/wc-calibration/tests/fixtures/regenerate_trf_path.py. It writes trf_path.json beside this file.
"""
import contextlib
import json
import os
import queue
import random
import sys

import scipy.optimize._lsq.trf as trf

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", "..", ".."))
sys.path.insert(0, ROOT)

import Actions.holeyCalibration as holeyCalibration  # noqa: E402

FIXTURE = os.path.join(ROOT, "tests", "fixtures", "calibration", "holey_calibrate.json")


class Config:
    def __init__(self, geometry):
        advanced = geometry["Advanced Settings"]
        dist_per_rot = float(advanced["gearTeeth"]) * float(advanced["chainPitch"])
        self.values = {**geometry, "Computed Settings": {"distPerRot": str(dist_per_rot)}}

    def getValue(self, section, key):
        return self.values[section].get(key)


class Data:
    def __init__(self, geometry):
        self.config = Config(geometry)
        self.console_queue = queue.Queue()
        self.message_queue = queue.Queue()


def recorded_path(calibration, measurements):
    iterations = []

    def record(iteration, nfev, cost, reduction, step_norm, optimality):
        iterations.append(
            {
                "iteration": iteration,
                "nfev": nfev,
                "cost": float(cost),
                "costReduction": None if reduction is None else float(reduction),
                "stepNorm": None if step_norm is None else float(step_norm),
                "optimality": float(optimality),
            }
        )

    least_squares = holeyCalibration.least_squares
    trf.print_iteration_nonlinear = record
    trf.print_header_nonlinear = lambda: None
    holeyCalibration.least_squares = lambda *args, **kwargs: least_squares(*args, verbose=2, **kwargs)
    try:
        form = {f"M{i + 1}": value for i, value in enumerate(measurements)}
        with open(os.devnull, "w") as quiet, contextlib.redirect_stdout(quiet):
            result = calibration.Calibrate(form)
    finally:
        holeyCalibration.least_squares = least_squares
    return iterations if result is not None else None


def stressed(measurements):
    """Gross errors, so the trust region rejects steps and shrinks: the paths the fixture cases never take."""
    rng = random.Random(5)
    noisy = [round(value + rng.uniform(-25.0, 25.0), 2) for value in measurements]
    blunder = [value + 60.0 if i in (0, 11) else value for i, value in enumerate(measurements)]
    return [("differing machine, uniform noise +-25 mm, seed 5", noisy), ("M1 and M12 60 mm long", blunder)]


def main():
    with open(FIXTURE) as infile:
        fixture = json.load(infile)
    calibration = holeyCalibration.HoleyCalibration()
    calibration.setUpData(Data(fixture["geometry"]))
    inputs = [(case["name"], case["measurements"]) for case in fixture["cases"] if case["expected"] is not None]
    inputs += stressed(fixture["cases"][1]["measurements"])
    cases = [
        {"name": name, "measurements": measurements, "iterations": recorded_path(calibration, measurements)}
        for name, measurements in inputs
    ]
    path = {
        "geometry": fixture["geometry"],
        "source": "scipy " + __import__("scipy").__version__ + " least_squares(method='trf'), verbose=2's columns",
        "cases": cases,
    }
    with open(os.path.join(HERE, "trf_path.json"), "w") as outfile:
        json.dump(path, outfile, indent=2)
        outfile.write("\n")


if __name__ == "__main__":
    main()
