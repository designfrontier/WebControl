"""Z-axis limits: only holey 51.292+ stores them, as $63/$64 in mm; B17-B19 corrupt or do nothing on every build."""
import pytest

from helpers import decode_ui_message, drain

REFUSAL = "Z-axis limits need holey firmware 51.292 or later"


@pytest.fixture
def firmware(data, monkeypatch):
    def set_firmware(version, rewrite_notice=False, units="MM", z=0.0):
        monkeypatch.setattr(data, "controllerFirmwareVersion", version)
        monkeypatch.setattr(data, "firmwareRewriteNoticeSeen", rewrite_notice)
        monkeypatch.setattr(data, "units", units)
        monkeypatch.setattr(data, "zval", z)
        monkeypatch.setattr(data, "uploadFlag", 0)

    return set_firmware


def act(data, command):
    data.actions.processAction({"data": {"command": command, "arg": "", "arg1": ""}})
    return drain(data.gcode_queue)


def alerts(data):
    return [message for command, _, message in map(decode_ui_message, drain(data.ui_queue1)) if command == "Alert"]


@pytest.mark.parametrize("version,rewrite_notice", [(0, False), (1.26, False), (51.27, False), (51.29, False), (100.1, False)])
@pytest.mark.parametrize("command", ["setMaxZ", "setMinZ", "clearZ", "getZlimits"])
def test_firmware_before_51_292_is_refused_with_the_reason(data, firmware, version, rewrite_notice, command):
    firmware(version, rewrite_notice)

    assert act(data, command) == []
    assert [REFUSAL in message for message in alerts(data)] == [True]


@pytest.mark.parametrize("units,z,line", [("MM", 12.34, "$63=12.340"), ("INCHES", 0.5, "$63=12.700")])
def test_saving_the_max_writes_the_current_z_in_mm(data, firmware, units, z, line):
    firmware(51.29, rewrite_notice=True, units=units, z=z)

    assert act(data, "setMaxZ") == [line]


def test_saving_the_min_writes_the_lower_limit(data, firmware):
    firmware(51.293, z=-20)

    assert act(data, "setMinZ") == ["$64=-20.000"]


def test_clearing_sets_limits_far_beyond_any_travel_and_reading_uses_b20(data, firmware):
    firmware(51.29, rewrite_notice=True)

    assert act(data, "clearZ") == ["$63=9999", "$64=-9999"]
    assert act(data, "getZlimits") == ["B20"]
    assert alerts(data) == []


@pytest.mark.parametrize("version,rewrite_notice,shown", [(51.27, False, False), (51.29, True, True)])
def test_the_classic_actions_page_shows_the_limits_button_only_on_51_292(app, data, firmware, version, rewrite_notice, shown):
    firmware(version, rewrite_notice)

    with app.test_request_context():
        page = app.webPageProcessor.createWebPage("actions", False, None)[0]

    button = next(line for line in page.splitlines() if "Set Z-Axis Limit" in line)
    assert ("hidden" not in button) is shown
