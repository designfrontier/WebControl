"""Checks tests/fixtures/compute/cases.json, which rust/crates/wc-firmware/tests/compute.rs matches, is what
config.py's computeSettings still makes; rerun tests/fixtures/compute/generate.py if the change is meant."""
import importlib.util
import itertools
import json
from pathlib import Path

from app import app

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "tests" / "fixtures" / "compute"


def load_generator():
    spec = importlib.util.spec_from_file_location("compute_generate", FIXTURES / "generate.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_the_compute_fixture_is_current(monkeypatch, tmp_path):
    monkeypatch.chdir(ROOT)
    monkeypatch.setenv("HOME", str(tmp_path))
    monkeypatch.setattr(app, "data", getattr(app, "data", None), raising=False)
    generator = load_generator()
    homes = itertools.count()
    monkeypatch.setattr(generator.tempfile, "mkdtemp", lambda: str(tmp_path / f"home{next(homes)}"))
    assert generator.cases() == json.loads((FIXTURES / "cases.json").read_text())
