"""A large gcode load must leave the gevent hub free: other greenlets keep ticking and a Stop still reaches the port."""
import json
import os
import subprocess
import sys

import pytest

PROBE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "gcode_load_probe.py")
LINES = 200_000


@pytest.fixture(scope="module")
def probe(tmp_path_factory):
    # conftest disables gevent patching for this process, so the probe runs patched in a child of its own.
    result = tmp_path_factory.mktemp("probe") / "result.json"
    subprocess.run([sys.executable, PROBE, str(LINES), str(result)], check=True, capture_output=True, timeout=300)
    return json.loads(result.read_text())


def test_the_large_file_loads(probe):
    assert probe["loaded"] is True
    assert probe["lines"] == LINES


def test_other_greenlets_keep_running_during_the_parse(probe):
    assert probe["maxGapSeconds"] < 0.1


def test_a_stop_queued_during_the_parse_reaches_the_serial_port_promptly(probe):
    assert probe["stopLatencySeconds"] is not None
    assert probe["stopLatencySeconds"] < 0.15
