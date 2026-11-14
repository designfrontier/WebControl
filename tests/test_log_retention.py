import glob
import os
import time

import pytest

import DataStructures.logger as logger_module
from DataStructures.logger import Logger
from File.retention import keepNewest


@pytest.fixture
def home(tmp_path, monkeypatch):
    monkeypatch.setenv("HOME", str(tmp_path))
    (tmp_path / ".WebControl").mkdir()
    return tmp_path / ".WebControl"


def test_starting_the_logger_starts_fresh_logs_and_keeps_the_last_session(home):
    (home / "log.txt").write_text("old session\n")
    (home / "alog.txt").write_text("old abbreviated session\n")
    Logger()
    assert (home / "log.txt").read_text() == ""
    assert (home / "alog.txt").read_text() == ""
    assert (home / "log.txt.1").read_text() == "old session\n"
    assert (home / "alog.txt.1").read_text() == "old abbreviated session\n"


def test_a_log_over_the_size_cap_is_rotated(home, monkeypatch):
    monkeypatch.setattr(logger_module, "MAX_LOG_BYTES", 100)
    logger = Logger()
    logger.writeToFile("x" * 80, True)
    logger.writeToFile("y" * 80, True)
    logger.writeToFile("z" * 10, True)
    assert (home / "log.txt.1").read_text() == "x" * 80
    assert (home / "log.txt").read_text() == "y" * 80 + "z" * 10


def test_logs_never_grow_past_two_files_per_log(home, monkeypatch):
    monkeypatch.setattr(logger_module, "MAX_LOG_BYTES", 100)
    logger = Logger()
    for _ in range(50):
        logger.writeToFile("a" * 60, False)
    assert sorted(os.listdir(home)) == ["alog.txt", "alog.txt.1", "log.txt"]
    assert all(os.path.getsize(home / name) <= 100 for name in os.listdir(home))


def test_clearing_logs_removes_the_rotated_files_too(home):
    (home / "log.txt.1").write_text("old")
    logger = Logger()
    logger.writeToFile("now", True)
    logger.writeToFile("now", False)
    assert logger.deleteLogFiles() is True
    assert os.listdir(home) == []


def test_keep_newest_removes_all_but_the_newest(tmp_path):
    for index in range(6):
        path = tmp_path / f"wc_backup_{index}.zip"
        path.write_text("")
        os.utime(path, (time.time() - 100 + index, time.time() - 100 + index))
    (tmp_path / "other.zip").write_text("")
    keepNewest(str(tmp_path / "wc_backup_*.zip"), 3)
    assert sorted(os.listdir(tmp_path)) == ["other.zip", "wc_backup_3.zip", "wc_backup_4.zip", "wc_backup_5.zip"]


@pytest.fixture
def zips(data):
    yield data.config.getHome()
    for pattern in ("wc_backup_*.zip", ".WebControl/wc_diagnostics_*.zip"):
        for path in glob.glob(os.path.join(data.config.getHome(), pattern)):
            os.remove(path)


@pytest.mark.parametrize(
    "make, pattern",
    [("backupWebControl", "wc_backup_*.zip"), ("downloadDiagnostics", ".WebControl/wc_diagnostics_*.zip")],
)
def test_backups_and_diagnostics_keep_only_the_newest(data, zips, monkeypatch, make, pattern):
    import Actions.actions as actions_module

    for index in range(8):
        stale = os.path.join(zips, pattern.replace("*", f"2000010{index}-000000"))
        open(stale, "w").close()
        os.utime(stale, (index, index))
    assert getattr(data.actions, make)()
    assert len(glob.glob(os.path.join(zips, pattern))) == actions_module.KEEP_ARCHIVES
