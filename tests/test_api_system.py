from types import SimpleNamespace

import pytest

from api.system import clean_markdown, pid_test_action, releases_payload, system_info


def fake_data(**over):
    values = {
        "pyInstallCurrentVersion": 0.95,
        "platform": "RPI",
        "controllerFirmwareVersion": 51.27,
        "stockFirmwareVersion": "1.26",
        "holeyFirmwareVersion": "51.28",
        "customFirmwareVersion": None,
        "pyInstallUpdateAvailable": False,
        "pyInstallUpdateVersion": 0,
    }
    return SimpleNamespace(**{**values, **over})


def release(tag, title="", body=""):
    return SimpleNamespace(tag_name=tag, title=title, body=body)


class TestSystemInfo:
    def test_reports_versions_and_capabilities(self):
        assert system_info(fake_data()) == {
            "version": "0.95",
            "platform": "RPI",
            "controllerFirmwareVersion": 51.27,
            "firmware": {"stock": "1.26", "holey": "51.28", "custom": None},
            "canTurnOff": True,
            "fourMotor": False,
            "update": {"available": False, "version": None},
        }

    @pytest.mark.parametrize("platform, can_turn_off", [("RPI", True), ("PYINSTALLER", True), ("WINDOWS", False)])
    def test_only_pi_and_pyinstaller_builds_can_turn_off(self, platform, can_turn_off):
        assert system_info(fake_data(platform=platform))["canTurnOff"] is can_turn_off

    def test_custom_firmware_has_four_motors(self):
        assert system_info(fake_data(controllerFirmwareVersion=100.1))["fourMotor"] is True

    def test_unknown_firmware_version_counts_as_zero(self):
        assert system_info(fake_data(controllerFirmwareVersion="?"))["controllerFirmwareVersion"] == 0.0

    def test_names_the_available_update(self):
        info = system_info(fake_data(pyInstallUpdateAvailable=True, pyInstallUpdateVersion=release("v0.96")))
        assert info["update"] == {"available": True, "version": "v0.96"}

    def test_endpoint_serves_the_live_data(self, client, data):
        body = client.get("/api/system/info").get_json()
        assert body["version"] == str(data.pyInstallCurrentVersion)
        assert body["platform"] == data.platform


class TestPid:
    def test_endpoint_reads_the_gains_from_config(self, client, data):
        body = client.get("/api/system/pid").get_json()
        assert body["velocity"]["kp"] == str(data.config.getValue("Advanced Settings", "KpV"))
        assert body["position"]["kd"] == str(data.config.getValue("Advanced Settings", "KdPos"))
        assert body["fourMotor"] is False

    def test_builds_the_velocity_message_the_old_ui_sent(self):
        body = {"kind": "velocity", "motor": "L", "start": 1, "stop": "20", "steps": 5, "version": "2"}
        body.update(kp="1300", ki="0.5", kd=" 0 ")
        assert pid_test_action(body) == {
            "data": {
                "command": "executeVelocityPIDTest",
                "arg": {
                    "vMotor": "L",
                    "vStart": "1",
                    "vStop": "20",
                    "vSteps": "5",
                    "vVersion": "2",
                    "KpV": "1300",
                    "KiV": "0.5",
                    "KdV": "0",
                },
                "arg1": "",
            }
        }

    def test_builds_the_position_message_with_time(self):
        body = {"kind": "position", "motor": "N", "start": 0, "stop": 10, "steps": 2, "time": 3, "version": 1}
        body.update(kp=1, ki=2, kd=3)
        arg = pid_test_action(body)["data"]["arg"]
        assert arg == {
            "pMotor": "N",
            "pStart": "0",
            "pStop": "10",
            "pSteps": "2",
            "pTime": "3",
            "pVersion": "1",
            "KpP": "1",
            "KiP": "2",
            "KdP": "3",
        }

    @pytest.mark.parametrize(
        "body, message",
        [
            ({"kind": "speed"}, "kind"),
            ({"kind": "velocity", "motor": "Z"}, "motor"),
            ({"kind": "position", "motor": "L", "start": "x"}, "not numbers: start, stop, steps, time"),
            ({"kind": "velocity", "motor": "L", "start": 0, "stop": 1, "steps": 1, "version": 1, "kp": "nan", "ki": 0, "kd": 0}, "not numbers: kp"),
            ({"kind": "velocity", "motor": "L", "start": 0, "stop": 1, "steps": 1, "version": 1, "kp": "inf", "ki": 0, "kd": 0}, "not numbers: kp"),
            ({"kind": "velocity", "motor": "L", "start": 0, "stop": 1, "steps": 1, "version": 1, "kp": "-inf", "ki": 0, "kd": 0}, "not numbers: kp"),
            ({"kind": "velocity", "motor": "L", "start": 0, "stop": 1, "steps": 1, "version": 1, "kp": "1e400", "ki": 0, "kd": 0}, "not numbers: kp"),
            ({"kind": "velocity", "motor": "L", "start": 0, "stop": 1, "steps": 1, "version": 1, "kp": "1_0", "ki": 0, "kd": 0}, "not numbers: kp"),
            ({"kind": "velocity", "motor": "L", "start": 0, "stop": 1, "steps": 1, "version": 1, "kp": True, "ki": 0, "kd": 0}, "not numbers: kp"),
            ({"kind": "velocity", "motor": "L", "start": 0, "stop": 1, "steps": 1, "version": 1, "kp": " ", "ki": 0, "kd": 0}, "not numbers: kp"),
            ({"kind": "velocity", "motor": "L", "start": 0, "stop": 1, "steps": 1, "version": 1, "kp": None, "ki": 0, "kd": 0}, "not numbers: kp"),
        ],
    )
    def test_rejects_bad_requests(self, body, message):
        with pytest.raises(ValueError, match=message):
            pid_test_action(body)

    @pytest.mark.parametrize("value", ["-1.5", ".5", "2.", " 3 ", 4, 0.25])
    def test_accepts_plain_decimals(self, value):
        body = {"kind": "velocity", "motor": "L", "start": 0, "stop": 1, "steps": 1, "version": 1, "ki": 0, "kd": 0}
        body["kp"] = value
        assert pid_test_action(body)["data"]["arg"]["KpV"] == str(value).strip()

    def test_endpoint_queues_the_b13_command(self, client, data):
        data.uploadFlag = 0
        body = {"kind": "velocity", "motor": "R", "start": 0, "stop": 10, "steps": 3, "version": 1}
        body.update(kp=1, ki=0, kd=0)
        response = client.post("/api/system/pid-test", json=body)
        assert response.status_code == 202
        assert data.gcode_queue.get_nowait() == "B13 R1 S0 F10 I3 V1"

    def test_endpoint_rejects_invalid_bodies(self, client, data):
        response = client.post("/api/system/pid-test", json={"kind": "velocity"})
        assert response.status_code == 400
        assert "motor" in response.get_json()["error"]
        assert data.gcode_queue.empty()

    def test_endpoint_refuses_while_gcode_runs(self, client, data):
        data.uploadFlag = 1
        body = {"kind": "velocity", "motor": "L", "start": 0, "stop": 1, "steps": 1, "version": 1, "kp": 1}
        body.update(ki=1, kd=1)
        response = client.post("/api/system/pid-test", json=body)
        assert response.status_code == 409
        assert data.gcode_queue.empty()


class TestReleases:
    def manager(self, releases, latest=None):
        return SimpleNamespace(
            releases=releases, getReleases=lambda: releases, getLatestRelease=lambda: latest
        )

    def test_lists_releases_with_current_and_latest(self):
        releases = [release("v0.96", "Faster", "* fixes"), release("v0.95", None, None)]
        data = fake_data(releaseManager=self.manager(releases, releases[0]))
        assert releases_payload(data) == {
            "current": "v0.95",
            "latest": "v0.96",
            "releases": [
                {"tag": "v0.96", "title": "Faster", "body": "* fixes"},
                {"tag": "v0.95", "title": "v0.95", "body": ""},
            ],
        }

    def test_is_empty_before_github_was_reached(self):
        payload = releases_payload(fake_data(releaseManager=self.manager(None)))
        assert payload["releases"] == [] and payload["latest"] is None

    def test_endpoint(self, client, data, monkeypatch):
        monkeypatch.setattr(data, "releaseManager", self.manager([release("v1.0")]))
        assert client.get("/api/system/releases").get_json()["releases"][0]["tag"] == "v1.0"


class TestHelp:
    def test_serves_the_home_page_by_default(self, client):
        body = client.get("/api/system/help").get_json()
        assert body["path"] == "index.md"
        assert body["title"] == "Home"
        assert body["markdown"].lstrip().startswith("# WebControl")
        assert "![Screenshot](assets/Screenshot.png)" in body["markdown"]

    def test_serves_nested_pages_with_their_title(self, client):
        body = client.get("/api/system/help?path=Actions/Diagnostics-Maintenance/testMotorsEncoders.md").get_json()
        assert body["title"] == "Test Motors/Encoders"
        assert "{: .label" not in body["markdown"]

    def test_falls_back_to_the_file_name_without_a_title(self, client, data, tmp_path, monkeypatch):
        (tmp_path / "docs").mkdir()
        (tmp_path / "docs" / "plain.md").write_text("hello")
        monkeypatch.setattr(data, "platform", "PYINSTALLER")
        monkeypatch.setattr(data, "platformHome", str(tmp_path))
        assert client.get("/api/system/help?path=plain.md").get_json()["title"] == "plain"

    @pytest.mark.parametrize("path", ["nope.md", "../README.md", "_config.yml", "Actions", "/etc/passwd"])
    def test_refuses_anything_but_docs_markdown(self, client, path):
        assert client.get("/api/system/help", query_string={"path": path}).status_code == 404

    def test_serves_doc_images(self, client):
        response = client.get("/api/system/help/assets/Screenshot.png")
        assert response.status_code == 200
        assert response.mimetype == "image/png"

    @pytest.mark.parametrize("path", ["missing.png", "../index.md"])
    def test_refuses_missing_or_outside_assets(self, client, path):
        assert client.get("/api/system/help/assets/" + path).status_code == 404

    def test_cleans_jekyll_links_and_raw_html(self):
        text = (
            "See [Holey]({{ site.baseurl }}{% link Actions/Calibration-Setup/holeyCalibration.md %}).\n"
            '>[Unit]</br>\n<span style="color: #ff0000;">Careful</span> Â\n'
            '<img src="assets/a.png" alt="A" width="100%">'
        )
        assert clean_markdown(text) == (
            "See [Holey](Actions/Calibration-Setup/holeyCalibration.md).\n>[Unit]  \nCareful \n![A](assets/a.png)"
        )
