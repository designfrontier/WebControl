"""Writes cases.json: what config.py's computeSettings makes of varied webcontrol.json files, and of groundcontrol.ini
imports, for rust/crates/wc-firmware/tests/compute.rs. Run from the repository root:
.venv/bin/python tests/fixtures/compute/generate.py
"""
import json
import os
import queue
import sys
import tempfile
from types import SimpleNamespace

sys.path.insert(0, os.getcwd())

from app import app  # noqa: E402
from config.config import Config  # noqa: E402
from File.importFile import ImportFile  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
FIRMWARE = 51.27
OWNER = {
    "Maslow Settings": {
        "motorSpacingX": "3011.16237", "motorOffsetY": "433.68186", "bedWidth": "2438.4", "bedHeight": "1219.2",
        "sledWeight": "97.9", "sledWidth": "310", "sledHeight": "139", "sledCG": "79", "COMport": "/dev/ttyACM0",
    },
    "Advanced Settings": {
        "leftChainTolerance": "-0.06495", "rightChainTolerance": "-0.02111", "chainElasticity": "5.1685e-06",
        "rotationRadius": "138.4", "chainOverSprocket": "Bottom", "gearTeeth": "10", "chainPitch": "6.35",
        "kinematicsType": "Triangular", "homeX": "41.6763",
    },
}
GAINS = {"KpPos": "1400", "KiPos": "0.5", "KdPos": "40", "propWeight": "0.9", "KpPosZ": "1200", "KiPosZ": "0",
         "KdPosZ": "30", "propWeightZ": "1", "KpV": "7", "KiV": "0.1", "KdV": "0.3", "KpVZ": "6", "KiVZ": "0",
         "KdVZ": "0.25"}
STALE = {"kinematicsTypeComputed": "1", "distPerRot": "70", "KpPosMain": "1300", "KdVMain": 0.5,
         "chainOverSprocketComputed": "1", "fPWMComputed": 2.0}
GROUNDCONTROL_INI = """[Maslow Settings]
COMport = /dev/ttyACM1
zAxis = 1
bedWidth = 2438.4
bedHeight = 1219.2
motorOffsetY = 463.0
motorSpacingX = 3602.6
sledWidth = 310
sledHeight = 139
sledCG = 79
notAWebControlKey = 5

[Advanced Settings]
gearTeeth = 9
chainPitch = 6.35
kinematicsType = Quadrilateral
chainOverSprocket = Top
fPWM = 31,000Hz
enablePosPIDValues = 1
KpPos = 1250
KiPos = 0
KdPos = 30
propWeight = 1
KpV = 6
KiV = 0
KdV = 0.25

[Ground Control Settings]
centerCanvasOnResize = 0

[Background Settings]
backgroundFile =

[Computed Settings]
kinematicsTypeComputed = 2
distPerRot = 63.5
KpPosMain = 1300
chainOverSprocketComputed = 2
fPWMComputed = 3
units = MM
"""
CASES = [
    {"name": "defaults", "overrides": {}},
    {"name": "owner", "overrides": OWNER},
    {"name": "quadrilateral_top_feed", "overrides": {
        "Advanced Settings": {"kinematicsType": "Quadrilateral", "chainOverSprocket": "Top"}}},
    {"name": "nine_tooth_sprocket", "overrides": {"Advanced Settings": {"gearTeeth": "9", "chainPitch": "6.35"}}},
    {"name": "numeric_sprocket_values", "overrides": {"Advanced Settings": {"gearTeeth": 11, "chainPitch": 6.35}}},
    {"name": "custom_gains_enabled", "overrides": {"Advanced Settings": {"enablePosPIDValues": 1, **GAINS}}},
    {"name": "custom_gains_disabled", "overrides": {"Advanced Settings": {"enablePosPIDValues": 0, **GAINS}}},
    {"name": "gains_enabled_as_text", "overrides": {"Advanced Settings": {"enablePosPIDValues": "1", "KpPos": 1500}}},
    {"name": "pwm_31khz", "overrides": {"Advanced Settings": {"fPWM": "31,000Hz"}}},
    {"name": "pwm_4khz", "overrides": {"Advanced Settings": {"fPWM": "4,100Hz"}}},
    {"name": "pwm_unknown_and_feed_unknown", "overrides": {
        "Advanced Settings": {"fPWM": "1Hz", "chainOverSprocket": "Sideways"}}},
    {"name": "stale_computed_values", "overrides": {"Computed Settings": STALE}},
    {"name": "stale_computed_owner", "overrides": {**OWNER, "Computed Settings": STALE}},
    {"name": "groundcontrol_ini", "overrides": {}, "ini": GROUNDCONTROL_INI},
    {"name": "groundcontrol_ini_over_owner", "overrides": OWNER, "ini": GROUNDCONTROL_INI},
]


def stub():
    return SimpleNamespace(
        controllerFirmwareVersion=FIRMWARE,
        gcode_queue=queue.Queue(),
        console_queue=queue.Queue(),
        message_queue=queue.Queue(),
        holeyKinematics=SimpleNamespace(updateSetting=lambda key, value: None),
        logger=SimpleNamespace(setLoggingTimeout=lambda timeout: None),
        serialRecorder=SimpleNamespace(setEnabled=lambda enabled: None),
    )


def drained(lines):
    return [lines.get_nowait() for _ in range(lines.qsize())]


def changes(before, after):
    """Every option value that differs, JSON type included, as sorted [section, key, value]."""
    def values(settings):
        return {(section, option["key"]): option.get("value") for section, options in settings.items()
                for option in options}
    old, new = values(before), values(after)
    return [[section, key, value] for (section, key), value in sorted(new.items())
            if json.dumps(old.get((section, key), "<missing>")) != json.dumps(value)]


def snapshot(config):
    return json.loads(json.dumps(config.settings))


def run(case):
    home = tempfile.mkdtemp()
    os.environ["HOME"] = home
    with open("defaultwebcontrol.json") as infile:
        settings = json.load(infile)
    for section, values in case["overrides"].items():
        for option in settings[section]:
            if option["key"] in values:
                option["value"] = values[option["key"]]
    os.makedirs(os.path.join(home, ".WebControl"))
    with open(os.path.join(home, ".WebControl", "webcontrol.json"), "w") as outfile:
        json.dump(settings, outfile)
    data = stub()
    app.data = data
    config = Config()
    config.data = data
    data.config = config
    before = snapshot(config)
    config.computeSettings(None, None, None, True)
    result = {"name": case["name"], "overrides": case["overrides"],
              "startup": {"changes": changes(before, snapshot(config)), "lines": drained(data.gcode_queue)}}
    if "ini" in case:
        result["ini"] = case["ini"]
        path = os.path.join(home, "groundcontrol.ini")
        with open(path, "w") as outfile:
            outfile.write(case["ini"])
        importer = ImportFile()
        importer.data = data
        before = snapshot(config)
        assert importer.importGCini(path), drained(data.console_queue)
        lines = drained(data.gcode_queue)
        assert lines[-1] == "$$"
        result["import"] = {"changes": changes(before, snapshot(config)), "lines": lines[:-1]}
    return result


def cases():
    return [run(case) for case in CASES]


if __name__ == "__main__":
    with open(os.path.join(HERE, "cases.json"), "w") as outfile:
        json.dump(cases(), outfile, indent=2, ensure_ascii=False)
        outfile.write("\n")
