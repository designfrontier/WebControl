"""Rewrites the JSON fixtures in this folder and in ../boards from the current Python code."""
import os
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))

if __name__ == "__main__":
    environment = {**os.environ, "REGENERATE_GCODE_FIXTURES": "1"}
    command = [sys.executable, "-m", "pytest", "-q", "tests/test_gcode_fixtures.py"]
    sys.exit(subprocess.call(command, cwd=ROOT, env=environment))
