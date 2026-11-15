"""The Python half of the shared ~/.WebControl round trip (spec/RUST-PLAN.md §7): the webcontrol.json the Rust backend
writes, tests/fixtures/rust_settings/rust-written.json (kept current by rust/crates/wc-config/tests/python_roundtrip.rs),
loads in config/config.py unchanged, and the same edits made through Config.setValue write the same file."""
import json
import os
import queue
import shutil
from pathlib import Path
from types import SimpleNamespace

import pytest

from app import app
from config.config import Config

FIXTURES = Path(__file__).parent / "fixtures" / "rust_settings"
# The edits rust/crates/wc-config/tests/python_roundtrip.rs makes, in its order.
EDITS = [
    ("Maslow Settings", "motorSpacingX", "3000"),
    ("Maslow Settings", "zAxis", True),
    ("Maslow Settings", "macro2_title", "Vide – poussière"),
    ("WebControl Settings", "webPort", "5001"),
    ("WebControl Settings", "loggingTimeout", "2.5"),
    ("Advanced Settings", "chainOverSprocket", "Top"),
    ("Advanced Settings", "enablePosPIDValues", True),
    ("Advanced Settings", "KpPos", "1400"),
    ("Maslow Settings", "sledWeight", "101.25"),
]


@pytest.fixture
def home(tmp_path, monkeypatch):
    monkeypatch.setenv("HOME", str(tmp_path))
    (tmp_path / ".WebControl").mkdir()
    return tmp_path / ".WebControl"


@pytest.fixture
def data(monkeypatch):
    """Just what setValue and computeSettings reach for, with nothing connected."""
    stub = SimpleNamespace(
        controllerFirmwareVersion=0,
        gcode_queue=queue.Queue(),
        console_queue=queue.Queue(),
        holeyKinematics=SimpleNamespace(updateSetting=lambda key, value: None),
        logger=SimpleNamespace(setLoggingTimeout=lambda timeout: None),
        serialRecorder=SimpleNamespace(setEnabled=lambda enabled: None),
    )
    monkeypatch.setattr(app, "data", stub, raising=False)
    return stub


def open_config(home, fixture, data):
    shutil.copy(FIXTURES / fixture, home / "webcontrol.json")
    config = Config()
    config.data = data
    return config


def test_the_rust_written_file_loads_without_falling_back(home, data):
    config = open_config(home, "rust-written.json", data)
    assert sorted(os.listdir(home)) == ["boards", "gcode", "imports", "webcontrol.json"]
    assert config.settings == json.loads((FIXTURES / "rust-written.json").read_text())
    assert config.getValue("WebControl Settings", "webPort") == 5001
    assert config.getValue("WebControl Settings", "loggingTimeout") == 2.5
    assert config.getValue("Maslow Settings", "zAxis") == 1
    assert config.getValue("Maslow Settings", "motorSpacingX") == "3000"
    assert config.getValue("Computed Settings", "KpPosMain") == 1400.0
    assert config.getValue("Advanced Settings", "maxZRPM") == "12.6"
    assert config.getValue("Some Plugin", "x") == [1, 2.5, None]


def test_python_writes_the_rust_written_file_back_byte_for_byte(home, data):
    config = open_config(home, "rust-written.json", data)
    config.saveSettings()
    assert (home / "webcontrol.json").read_bytes() == (FIXTURES / "rust-written.json").read_bytes()


def test_the_same_edits_through_set_value_write_the_same_file(home, data):
    config = open_config(home, "python-written.json", data)
    [config.setValue(section, key, value) for section, key, value in EDITS]
    config.adoptUnknownSetting(18, 12.6)
    assert (home / "webcontrol.json").read_text() == (FIXTURES / "rust-written.json").read_text()



def test_the_same_edits_send_the_controller_lines_rust_sends(home, data):
    config = open_config(home, "python-written.json", data)

    def lines_after(section, key, value):
        config.setValue(section, key, value)
        return [data.gcode_queue.get_nowait() for _ in range(data.gcode_queue.qsize())]

    sent = [lines_after(*edit) for edit in EDITS]
    assert sent == json.loads((FIXTURES / "edit-lines.json").read_text())
