"""Runs main.py against a throwaway ~/.WebControl so e2e runs never touch the real config."""
import atexit
import json
import os
import runpy
import shutil
import sys
import tempfile

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def prepare_home(port):
    home = tempfile.mkdtemp(prefix="webcontrol-e2e-")
    atexit.register(shutil.rmtree, home, ignore_errors=True)
    os.makedirs(os.path.join(home, ".WebControl"))
    with open(os.path.join(REPO, "defaultwebcontrol.json")) as infile:
        settings = json.load(infile)
    for option in settings["WebControl Settings"]:
        if option["key"] == "webPort":
            option["value"] = port
    with open(os.path.join(home, ".WebControl", "webcontrol.json"), "w") as outfile:
        json.dump(settings, outfile)
    return home


class NoCamera:
    """Stands in for cv2.VideoCapture so an e2e run never opens the host's camera."""

    def __init__(self, *args, **kwargs):
        pass

    def isOpened(self):
        return False

    def read(self):
        return False, None

    def get(self, prop):
        return 0.0

    def set(self, prop, value):
        return False

    def release(self):
        pass


def disable_camera():
    import cv2

    cv2.VideoCapture = NoCamera


if __name__ == "__main__":
    port = int(os.environ.get("E2E_BACKEND_PORT", "5055"))
    # main.py's webcam module opens camera 0 at startup to read its settings.
    disable_camera()
    os.environ["HOME"] = prepare_home(port)
    # webbrowser honours $BROWSER; `true` turns main.py's open-a-tab into a no-op.
    os.environ["BROWSER"] = "true"
    os.chdir(REPO)
    sys.path.insert(0, REPO)
    sys.argv = [os.path.join(REPO, "main.py")]
    runpy.run_path(sys.argv[0], run_name="__main__")
