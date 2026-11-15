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


def run_install_script(tmp_path, body, machine="armv7l", userland=None, codename="bullseye"):
    """Runs body in bash after sourcing tools/install_from_source.sh, with the platform faked and repo/ in tmp_path."""
    for name in ["requirements.txt", "requirements-pi-overrides.txt", "requirements-pi-apt.txt"]:
        shutil.copy(os.path.join(REPO, name), tmp_path / name)
    (tmp_path / "os-release").write_text(f"VERSION_CODENAME={codename}\n")
    script = os.path.join(REPO, "tools", "install_from_source.sh")
    prelude = f'source "{script}"; repo="{tmp_path}"; venv="$repo/.venv"; staged="$repo/build/webcontrol.service"; '
    prelude += f'os_release="{tmp_path}/os-release"; '
    prelude += f'uname() {{ [ "$1" = -m ] && echo {machine} || command uname "$@"; }}; '
    prelude += f"dpkg() {{ echo {userland}; }}; " if userland else "dpkg() { return 1; }; "
    prelude += 'sudo() { echo "sudo $*" >>"$repo/sudo.log"; }; '
    return subprocess.run(["bash", "-c", prelude + body], capture_output=True, text=True)


def source_install_script(tmp_path, body, **platform):
    result = run_install_script(tmp_path, body, **platform)
    assert result.returncode == 0, result.stderr
    return result.stdout


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


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
@pytest.mark.parametrize("codename", ["buster", "bookworm"])
def test_32_bit_pi_os_other_than_bullseye_stops_before_installing_anything(tmp_path, codename):
    result = run_install_script(tmp_path, "check_platform; echo carried on", codename=codename)
    assert result.returncode != 0
    assert "Bullseye" in result.stderr and "carried on" not in result.stdout


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
@pytest.mark.parametrize("machine, codename", [("armv7l", "bullseye"), ("aarch64", "bookworm"), ("aarch64", "buster")])
def test_bullseye_32_bit_and_any_64_bit_pi_os_pass_the_platform_check(tmp_path, machine, codename):
    assert "carried on" in source_install_script(
        tmp_path, "check_platform; echo carried on", machine=machine, codename=codename
    )


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_a_32_bit_userland_on_a_64_bit_kernel_counts_as_32_bit(tmp_path):
    result = run_install_script(tmp_path, "check_platform", machine="aarch64", userland="armhf", codename="bookworm")
    assert result.returncode != 0


def installed_packages(tmp_path):
    lines = (tmp_path / "sudo.log").read_text().splitlines()
    return next(line for line in lines if "apt-get install" in line).split()


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_32_bit_pi_os_gets_the_libraries_the_piwheels_wheels_link(tmp_path):
    source_install_script(tmp_path, "find_uv() { echo uv; }; apt_packages")
    packages = installed_packages(tmp_path)
    assert {"libavcodec58", "libgtk-3-0", "libatlas3-base", "libopenblas0-pthread", "libgfortran5"} <= set(packages)
    assert "libgl1" not in packages


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_64_bit_pi_os_gets_the_libraries_pypi_opencv_loads(tmp_path):
    source_install_script(tmp_path, "find_uv() { echo uv; }; apt_packages", machine="aarch64", userland="arm64")
    packages = installed_packages(tmp_path)
    assert {"libgl1", "libglib2.0-0"} <= set(packages)
    assert "libavcodec58" not in packages


def v094_unit_file(tmp_path, user_line):
    (tmp_path / "lib").mkdir(exist_ok=True)
    unit = tmp_path / "lib" / "webcontrol.service"
    unit.write_text(f"[Service]\nExecStart=/home/pi/webcontrol/webcontrol\n{user_line}\n")
    return unit


def fake_systemd(tmp_path, fragment, active=()):
    """Bash that fakes systemctl: webcontrol.service loads from fragment, and the units in active are running."""
    actives = " ".join(active)
    return (
        f'old_unit="{tmp_path}/webcontrol-v094.service"; '
        f'systemctl() {{ case "$*" in "show -p FragmentPath --value webcontrol.service") echo "{fragment}" ;; '
        f'"is-active --quiet "*|"is-enabled --quiet "*) case " {actives} " in *" ${{@: -1}} "*) return 0 ;; esac; return 1 ;; '
        f'*) return 1 ;; esac; }}; port_busy() {{ return 1; }}; '
    )


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_a_v094_unit_outside_etc_is_kept_for_rollback_and_left_running(tmp_path):
    unit = v094_unit_file(tmp_path, f"User={subprocess.check_output(['id', '-un'], text=True).strip()}")
    source_install_script(tmp_path, fake_systemd(tmp_path, unit, active=["webcontrol"]) + "install_unit")
    log = (tmp_path / "sudo.log").read_text()
    assert f"sudo cp {unit} {tmp_path}/webcontrol-v094.service" in log
    assert "install -m 644" not in log and "restart" not in log


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
@pytest.mark.parametrize("user_line", ["User=somebody-else", ""])
def test_refuses_to_run_as_a_different_user_than_v094(tmp_path, user_line):
    unit = v094_unit_file(tmp_path, user_line)
    result = run_install_script(tmp_path, fake_systemd(tmp_path, unit) + "check_same_user")
    assert result.returncode != 0
    assert "/.WebControl" in result.stderr


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_runs_as_the_same_user_as_v094(tmp_path):
    unit = v094_unit_file(tmp_path, f"User={subprocess.check_output(['id', '-un'], text=True).strip()}")
    source_install_script(tmp_path, fake_systemd(tmp_path, unit) + "check_same_user")


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_updating_a_running_service_does_not_restart_it_mid_cut(tmp_path):
    source_install_script(tmp_path, fake_systemd(tmp_path, "", active=["webcontrol"]) + "install_unit")
    log = (tmp_path / "sudo.log").read_text()
    assert "install -m 644" in log
    assert "restart" not in log and "enable" not in log


def activate_log(tmp_path, active):
    body = fake_systemd(tmp_path, "", active=active) + "wait_for_server() { :; }; write_unit; activate"
    source_install_script(tmp_path, body)
    return (tmp_path / "sudo.log").read_text().splitlines()


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_activate_stops_a_v094_copy_that_was_started_by_hand(tmp_path):
    assert "sudo systemctl disable --now webcontrol-v094" in activate_log(tmp_path, active=["webcontrol-v094"])


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_activate_restarts_the_new_service_onto_updated_code(tmp_path):
    log = activate_log(tmp_path, active=["webcontrol"])
    assert "sudo systemctl restart webcontrol" in log
    assert not any("webcontrol-v094" in line for line in log)


def settings_prelude(tmp_path):
    (tmp_path / "home").mkdir(exist_ok=True)
    return f'settings="{tmp_path}/home/webcontrol.json"; settings_v094="$settings.v094"; wait_for_server() {{ :; }}; '


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_activating_from_v094_saves_its_settings_for_rollback(tmp_path):
    unit = v094_unit_file(tmp_path, f"User={subprocess.check_output(['id', '-un'], text=True).strip()}")
    body = settings_prelude(tmp_path) + fake_systemd(tmp_path, unit, active=["webcontrol"]) + "write_unit; activate"
    (tmp_path / "home" / "webcontrol.json").write_text('{"v094": true}')
    source_install_script(tmp_path, body)
    assert (tmp_path / "home" / "webcontrol.json.v094").read_text() == '{"v094": true}'


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_restarting_the_new_service_keeps_the_v094_settings_it_saved(tmp_path):
    body = settings_prelude(tmp_path) + fake_systemd(tmp_path, "", active=["webcontrol"]) + "write_unit; activate"
    (tmp_path / "home" / "webcontrol.json").write_text('{"next": true}')
    (tmp_path / "home" / "webcontrol.json.v094").write_text('{"v094": true}')
    (tmp_path / "webcontrol-v094.service").write_text("[Service]\n")
    source_install_script(tmp_path, body)
    assert (tmp_path / "home" / "webcontrol.json.v094").read_text() == '{"v094": true}'


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_rollback_starts_v094_on_the_settings_it_last_saved(tmp_path):
    body = settings_prelude(tmp_path) + fake_systemd(tmp_path, "", active=["webcontrol"]) + "rollback"
    (tmp_path / "home" / "webcontrol.json").write_text('{"next": true, "maxZRPM": ""}')
    (tmp_path / "home" / "webcontrol.json.v094").write_text('{"v094": true}')
    (tmp_path / "webcontrol-v094.service").write_text("[Service]\n")
    source_install_script(tmp_path, body)
    assert (tmp_path / "home" / "webcontrol.json").read_text() == '{"v094": true}'
    assert (tmp_path / "home" / "webcontrol.json.next").read_text() == '{"next": true, "maxZRPM": ""}'
    assert (tmp_path / "sudo.log").read_text().splitlines() == [
        "sudo systemctl disable --now webcontrol",
        "sudo systemctl enable --now webcontrol-v094",
    ]


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_rollback_needs_the_saved_v094_unit(tmp_path):
    result = run_install_script(tmp_path, settings_prelude(tmp_path) + fake_systemd(tmp_path, "") + "rollback")
    assert result.returncode != 0
    assert not (tmp_path / "sudo.log").exists()


def test_the_pi_guide_copies_no_firmware_so_upgrade_firmware_has_nothing_to_flash():
    with open(os.path.join(REPO, "spec", "INSTALL-PI.md")) as handle:
        guide = handle.read()
    rsync = guide[guide.index("rsync -az"):guide.index("pi@192.168.86.155:webcontrol-source/")]
    assert "--exclude /firmware" in rsync
