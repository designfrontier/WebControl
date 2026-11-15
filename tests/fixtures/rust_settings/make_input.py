"""Writes python-written.json: a webcontrol.json as the Python backend saves it, with the kinds of values a real one
holds (a section and fields no WebControl knows, non-ASCII text, a number in a text setting, the settings screens'
"default setting:" descriptions). Run from the repository root: .venv/bin/python tests/fixtures/rust_settings/make_input.py
"""
import os
import shutil
import sys
import tempfile

sys.path.insert(0, os.getcwd())
os.environ["HOME"] = tempfile.mkdtemp()

from config.config import Config  # noqa: E402

config = Config()
config.data = None
settings = config.settings


def option(section, key):
    return next(o for o in settings[section] if o["key"] == key)


settings["Maslow Settings"].append({"key": "legacyOption", "value": "kept", "fromAnOlderWebControl": True})
option("Maslow Settings", "macro1_title")["value"] = "Broche à étages – ok"
option("Advanced Settings", "homeX")["value"] = 12.5
settings["Some Plugin"] = [{"key": "x", "value": [1, 2.5, None], "nested": {"b": 1, "a": "z"}}]
config.getJSONSettingSection("Maslow Settings")
config.saveSettings()
here = os.path.dirname(os.path.abspath(__file__))
shutil.copy(config.settingsPath(), os.path.join(here, "python-written.json"))
