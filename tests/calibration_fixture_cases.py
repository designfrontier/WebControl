"""Builds the golden calibration fixtures in tests/fixtures/calibration from the running code."""
import math
import random

from Actions.HoleySimulationKinematics import Kinematics
from calibration_helpers import holey_form
from helpers import drain

GEOMETRY = {
    "Maslow Settings": {
        "bedWidth": "2438.4",
        "bedHeight": "1219.2",
        "motorSpacingX": "2978.4",
        "motorOffsetY": "463",
        "sledWeight": "97.9",
    },
    "Advanced Settings": {
        "rotationRadius": "139.1",
        "chainSagCorrection": "33",
        "leftChainTolerance": "0",
        "rightChainTolerance": "0",
        "chainOverSprocket": "Bottom",
        "gearTeeth": "10",
        "chainPitch": "6.35",
        "chainElasticity": "5.1685e-6",
        "kinematicsType": "Triangular",
    },
}
KINEMATIC_PARAMETERS = [
    "D",
    "R",
    "machineWidth",
    "machineHeight",
    "motorOffsetY",
    "rotationDiskRadius",
    "sledWeight",
    "chainDensity",
    "chainElasticity",
    "leftChainTolerance",
    "rightChainTolerance",
    "chainOverSprocket",
    "l",
    "s",
    "h3",
    "isQuadKinematics",
]
FIRMWARE_UNITS = {0: "mm", 1: "mm", 2: "mm", 3: "mm", 8: "mm", 13: "mm", 37: "", 40: "%", 41: "%"}
OPTIMISED_PARAMETERS = ["Opt_D", "Opt_motorOffsetY", "Opt_leftChainTolerance", "Opt_rightChainTolerance"]
TRUE_MACHINE = {"D": 2990.0, "motorOffsetY": 470.0, "leftChainTolerance": 0.3, "rightChainTolerance": -0.2}
TRIANGULAR_INPUTS = [
    {"cut12": 1930, "cut34": 1930, "cut5": 300, "bitDiameter": 6.35},
    {"cut12": 1925.5, "cut34": 1932, "cut5": 310, "bitDiameter": 6.35},
    {"cut12": 1940, "cut34": 1921.5, "cut5": 290.5, "bitDiameter": 3.175},
    {"cut12": 3000, "cut34": 1930, "cut5": 300, "bitDiameter": 6.35},
]


def apply_geometry(data):
    for section, values in GEOMETRY.items():
        for key, value in values.items():
            data.config.setValue(section, key, value)
    drain(data.gcode_queue)


def kinematic_parameters(kinematics):
    return {name: getattr(kinematics, name) for name in KINEMATIC_PARAMETERS}


def holey_result(data, measurements):
    result = data.holeyCalibration.Calibrate(holey_form(["" if value is None else value for value in measurements]))
    if result is None:
        return None
    motor_offset_y, distance, left, right, total_error = (float(value) for value in result)
    return {
        "Opt_D": distance,
        "Opt_motorOffsetY": motor_offset_y,
        "Opt_leftChainTolerance": left,
        "Opt_rightChainTolerance": right,
        "totalError": total_error,
    }


def true_machine_measurements(data):
    holey = data.holeyCalibration
    holey.InitializeIdealXyCoordinates()
    holey.SimulateMeasurement(**TRUE_MACHINE)
    return [float(value) for value in holey.MeasuredLengthArray]


def holey_cases(data):
    ideal = [float(value) for value in data.holeyCalibration.InitializeIdealXyCoordinates()]
    machine = true_machine_measurements(data)

    def noisy(base, seed, amplitude):
        rng = random.Random(seed)
        return [round(value + rng.uniform(-amplitude, amplitude), 2) for value in base]

    inputs = [
        ("ideal measurements of the configured geometry", ideal),
        ("exact measurements of a machine that differs from the configuration", machine),
        ("configured geometry, uniform noise +-0.5 mm, seed 1", noisy(ideal, 1, 0.5)),
        ("configured geometry, uniform noise +-1 mm, seed 2", noisy(ideal, 2, 1.0)),
        ("differing machine, uniform noise +-0.5 mm, seed 3", noisy(machine, 3, 0.5)),
        ("differing machine, uniform noise +-1 mm, seed 4", noisy(machine, 4, 1.0)),
        ("M5 missing: refused", [*ideal[:4], None, *ideal[5:]]),
        ("M12 not a number: refused", [*ideal[:11], "abc"]),
    ]
    cases = [{"name": name, "measurements": values, "expected": holey_result(data, values)} for name, values in inputs]
    data.holeyCalibration.InitializeIdealXyCoordinates()
    return {
        "geometry": GEOMETRY,
        "kinematics": kinematic_parameters(data.holeyCalibration.kin),
        "trueMachine": TRUE_MACHINE,
        "tolerance": 1e-6,
        "keyTolerance": {key: 1e-3 for key in OPTIMISED_PARAMETERS},
        "cases": cases,
    }


def simulator(quadrilateral):
    kinematics = Kinematics()
    kinematics.D = 2978.4
    kinematics.R = 10 * 6.35 / math.pi / 2
    kinematics.machineWidth = 2438.4
    kinematics.machineHeight = 1219.2
    kinematics.motorOffsetY = 463.0
    kinematics.rotationDiskRadius = 139.1
    kinematics.sledWeight = 97.9
    kinematics.chainElasticity = 5.1685e-6
    kinematics.chainOverSprocket = 2
    kinematics.isQuadKinematics = quadrilateral
    kinematics.recomputeGeometry()
    return kinematics


def grid():
    xs = [round(-1219.2 + 304.8 * n, 1) for n in range(9)]
    ys = [round(-609.6 + 152.4 * n, 1) for n in range(9)]
    return [(x, y) for y in ys for x in xs]


def kinematics_points(kinematics):
    def point(x, y):
        left, right = kinematics.inverse(x, y)
        position = kinematics.forward(left, right)
        return {"x": x, "y": y, "left": left, "right": right, "forward": None if position is None else list(position)}

    return [point(x, y) for x, y in grid()]


OFF_BED = [(-5000.0, 5000.0), (5000.0, -5000.0), (-1500.0, 0.0), (0.0, 700.0)]
UNREACHABLE = [(100.0, 100.0), (4000.0, 100.0)]


def off_bed_points(kinematics):
    return [{"x": x, "y": y, "chains": list(kinematics.inverse(x, y))} for x, y in OFF_BED]


def unreachable_points(kinematics):
    return [{"left": left, "right": right, "forward": kinematics.forward(left, right)} for left, right in UNREACHABLE]


def kinematics_cases():
    def mode(name, kinematics):
        points = kinematics_points(kinematics)
        return {
            "mode": name,
            "parameters": kinematic_parameters(kinematics),
            "points": points,
            "offBed": off_bed_points(kinematics),
            "unreachable": unreachable_points(kinematics),
        }

    return {
        "tolerance": 1e-9,
        "forwardGuess": [-10, 10],
        "modes": [mode("triangular", simulator(False)), mode("quadrilateral", simulator(True))],
    }


def triangular_cases(data):
    def case(inputs):
        result = data.triangularCalibration.calculate(inputs)
        keys = ["motorYoffsetEst", "rotationRadiusEst", "chainSagCorrectionEst", "cut34YoffsetEst"]
        return {"inputs": inputs, "expected": None if result is None else dict(zip(keys, map(float, result)))}

    return {"geometry": GEOMETRY, "tolerance": 1e-6, "cases": [case(inputs) for inputs in TRIANGULAR_INPUTS]}


def queued_commands(data):
    def describe(line):
        if not line.startswith("$"):
            return {"gcode": line}
        key, value = line[1:].split("=", 1)
        return {"command": line, "key": int(key), "value": value, "units": FIRMWARE_UNITS.get(int(key), "")}

    return [describe(line) for line in drain(data.gcode_queue)]


def accept_cases(data):
    holey = data.holeyCalibration
    holey.Calibrate(holey_form(true_machine_measurements(data)))
    holey.acceptCalibrationResults()
    holey_commands = queued_commands(data)
    apply_geometry(data)
    data.triangularCalibration.calculate(TRIANGULAR_INPUTS[1])
    data.triangularCalibration.acceptTriangularCalibrationResults()
    return {
        "geometry": GEOMETRY,
        "holey": {"measurements": "exact measurements of the differing machine", "trueMachine": TRUE_MACHINE, "queued": holey_commands},
        "triangular": {"inputs": TRIANGULAR_INPUTS[1], "queued": queued_commands(data)},
    }
