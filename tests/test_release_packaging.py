import os
import shutil
import subprocess

import pytest

from conftest import REPO
from pyinstaller_datas import MISSING_DIST, bundle_datas

SPECS = ["main.spec", "main-onedir.spec"]
SCRIPTS = ["tools/build_release.sh", "tools/install_from_source.sh", "buildLinux1.sh"]


@pytest.fixture
def tree(tmp_path):
    for name in ["templates", "static", "tools", "docs", "frontend/dist"]:
        (tmp_path / name).mkdir(parents=True)
    (tmp_path / "defaultwebcontrol.json").write_text("{}")
    (tmp_path / "frontend" / "dist" / "index.html").write_text("<div id=root></div>")
    return tmp_path


def destinations(datas):
    return {os.path.basename(source): destination for source, destination in datas}


def test_bundles_the_frontend_build_under_frontend_dist(tree):
    datas = bundle_datas(str(tree), log=lambda message: None)
    assert (str(tree / "frontend" / "dist"), os.path.join("frontend", "dist")) in datas
    assert destinations(datas)["defaultwebcontrol.json"] == "."
    assert destinations(datas)["templates"] == "templates"


def test_refuses_to_build_without_the_frontend(tree):
    (tree / "frontend" / "dist" / "index.html").unlink()
    with pytest.raises(SystemExit, match="npm run build"):
        bundle_datas(str(tree))
    assert "frontend/dist" in MISSING_DIST


def test_leaves_contract_out_because_only_the_tests_read_it(tree):
    (tree / "contract").mkdir()
    assert "contract" not in destinations(bundle_datas(str(tree), log=lambda message: None))


def test_firmware_is_bundled_when_built_and_warned_about_when_not(tree):
    warnings = []
    assert "firmware" not in destinations(bundle_datas(str(tree), log=warnings.append))
    assert any("firmware" in warning for warning in warnings)
    (tree / "firmware").mkdir()
    assert destinations(bundle_datas(str(tree)))["firmware"] == "firmware"


@pytest.mark.parametrize("spec", SPECS)
def test_specs_use_the_shared_datas_and_no_machine_paths(spec):
    with open(os.path.join(REPO, spec)) as handle:
        text = handle.read()
    assert "bundle_datas(SPECPATH)" in text
    assert "C:\\\\" not in text and "Users" not in text


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
@pytest.mark.parametrize("script", SCRIPTS)
def test_release_scripts_parse(script):
    assert subprocess.run(["bash", "-n", os.path.join(REPO, script)]).returncode == 0


def test_buildLinux1_delegates_to_the_release_script():
    with open(os.path.join(REPO, "buildLinux1.sh")) as handle:
        assert "tools/build_release.sh" in handle.read()


def source_install_script(tmp_path, body, machine="armv7l"):
    """Runs body in bash after sourcing tools/install_from_source.sh with uname -m faked and repo/ in tmp_path."""
    for name in ["requirements.txt", "requirements-pi-overrides.txt"]:
        shutil.copy(os.path.join(REPO, name), tmp_path / name)
    script = os.path.join(REPO, "tools", "install_from_source.sh")
    prelude = f'source "{script}"; repo="{tmp_path}"; venv="$repo/.venv"; staged="$repo/build/webcontrol.service"; '
    prelude += f'uname() {{ [ "$1" = -m ] && echo {machine} || command uname "$@"; }}; '
    return subprocess.run(["bash", "-c", prelude + body], capture_output=True, text=True, check=True).stdout


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_armv7_swaps_in_the_pi_overrides(tmp_path):
    lines = open(source_install_script(tmp_path, "effective_requirements").strip()).read().splitlines()
    assert "scipy==1.13.1" in lines
    assert not any(line.startswith("scipy==1.9") for line in lines)
    assert "numpy==1.23.5" in lines


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_other_machines_install_requirements_unchanged(tmp_path):
    path = source_install_script(tmp_path, "effective_requirements", machine="aarch64").strip()
    assert open(path).read() == open(os.path.join(REPO, "requirements.txt")).read()


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_the_service_runs_main_from_the_venv_as_the_invoking_user(tmp_path):
    unit = source_install_script(tmp_path, 'write_unit; cat "$staged"')
    assert unit.startswith("# Installed by WebControl tools/install_from_source.sh")
    assert f"ExecStart={tmp_path}/.venv/bin/python {tmp_path}/main.py" in unit
    assert f"WorkingDirectory={tmp_path}" in unit
    assert f"User={subprocess.check_output(['id', '-un'], text=True).strip()}" in unit
    assert "Restart=on-failure" in unit
