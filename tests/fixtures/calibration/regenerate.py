"""Rewrites the JSON fixtures in this folder from the current Python code."""
import os
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))

if __name__ == "__main__":
    environment = {**os.environ, "REGENERATE_CALIBRATION_FIXTURES": "1"}
    command = [sys.executable, "-m", "pytest", "-q", "tests/test_calibration_fixtures.py"]
    sys.exit(subprocess.call(command, cwd=ROOT, env=environment))
