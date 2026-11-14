import os
import re
import shutil
import stat
import subprocess

import pytest

import Actions.firmware as firmware
from conftest import REPO

SCRIPT = os.path.join(REPO, "buildFirmware.sh")
BASH = shutil.which("bash")

FAKE_GIT = """#!/bin/sh
if [ "$1" = clone ]; then
  dest="$4"
  mkdir -p "$dest/cnc_ctrl_v1"
  echo "#define VERSIONNUMBER 1.26" > "$dest/cnc_ctrl_v1/Maslow.h"
fi
echo "git $*" >> "$FAKE_LOG"
"""

FAKE_PIO = """#!/bin/sh
echo "pio $* in $(basename "$PWD")" >> "$FAKE_LOG"
if [ "$1" = run ] && [ "$(basename "$PWD")" != "$FAKE_PIO_FAIL" ]; then
  mkdir -p .pio/build/megaatmega2560
  echo ":00000001FF" > .pio/build/megaatmega2560/firmware.hex
fi
"""


def executable(path, text):
    path.write_text(text)
    path.chmod(path.stat().st_mode | stat.S_IEXEC)


def run_build(tmp_path, fail=""):
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    executable(bin_dir / "git", FAKE_GIT)
    executable(bin_dir / "pio", FAKE_PIO)
    env = {
        **os.environ,
        "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
        "FIRMWARE_DIR": str(tmp_path / "firmware"),
        "BUILD_DIR": str(tmp_path / "build"),
        "FAKE_LOG": str(tmp_path / "log"),
        "FAKE_PIO_FAIL": fail,
    }
    return subprocess.run([BASH, SCRIPT], env=env, capture_output=True, text=True)


def built(tmp_path):
    root = tmp_path / "firmware"
    return sorted(str(path.relative_to(root)) for path in root.rglob("*.hex"))


def test_the_build_script_parses():
    assert subprocess.run([BASH, "-n", SCRIPT]).returncode == 0


def test_the_build_script_writes_each_variant_where_upgrade_firmware_looks(tmp_path):
    result = run_build(tmp_path)
    assert result.returncode == 0, result.stderr
    assert built(tmp_path) == sorted(f"{folder}/{folder}-1.26.hex" for folder in firmware.FIRMWARE_FOLDERS.values())
    assert all(firmware.firmwareFiles(str(tmp_path), variant) for variant in firmware.FIRMWARE_FOLDERS)


def test_the_build_script_checks_out_the_pinned_firmware(tmp_path):
    run_build(tmp_path)
    log = (tmp_path / "log").read_text()
    for sha in (
        "bf4350ffd9bc154832505fc0125abd2c4c04dba7",
        "e1e0d020fff1f4f7c6b403a26a85a16546b7e15b",
        "950fb23396171cbd456c2d4149455cc45f5e6bc3",
    ):
        assert f"checkout --quiet {sha}" in log


def test_the_build_script_fails_when_a_variant_produces_no_hex(tmp_path):
    result = run_build(tmp_path, fail="holey")
    assert result.returncode != 0


def test_the_build_script_and_the_upgrade_share_one_layout():
    with open(SCRIPT) as handle:
        variants = re.search(r"^variants=\((.*)\)$", handle.read(), re.M).group(1).split()
    assert sorted(variants) == sorted(firmware.FIRMWARE_FOLDERS.values())


@pytest.mark.parametrize("path", ["buildLinux1.sh", "Dockerfile", ".travis.yml"])
def test_other_builds_call_the_one_build_script(path):
    with open(os.path.join(REPO, path)) as handle:
        assert "buildFirmware.sh" in handle.read()


def test_bundled_versions_are_read_from_the_same_layout(data, tmp_path, monkeypatch):
    for folder, version in (("madgrizzle", "1.27"), ("maslowcnc", "1.26"), ("holey", "51.28")):
        (tmp_path / "firmware" / folder).mkdir(parents=True)
        (tmp_path / "firmware" / folder / f"{folder}-{version}.hex").write_text("")
    saved = (data.customFirmwareVersion, data.stockFirmwareVersion, data.holeyFirmwareVersion)
    monkeypatch.setattr(firmware, "installRoot", lambda data: str(tmp_path))
    data.config.parseFirmwareVersions()
    found = (data.customFirmwareVersion, data.stockFirmwareVersion, data.holeyFirmwareVersion)
    data.customFirmwareVersion, data.stockFirmwareVersion, data.holeyFirmwareVersion = saved
    assert found == ("1.27", "1.26", "51.28")
