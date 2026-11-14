import os
from types import SimpleNamespace

import pytest

import ReleaseManager.releaseManager as release_module

ASSET_URL = "https://github.com/WebControlCNC/WebControl/releases/download/v0.99/webcontrol-0.99-linux-singlefile.tar.gz"


@pytest.fixture
def update(data, monkeypatch):
    """Self-update with the download, the GitHub request and the upgrade script all faked."""
    requested, launched = [], []

    def fake_get(url, **kwargs):
        requested.append(url)
        return SimpleNamespace(content=b"fake tarball", status_code=200, raise_for_status=lambda: None)

    monkeypatch.setattr(release_module.requests, "get", fake_get)
    monkeypatch.setattr(release_module.subprocess, "Popen", lambda command: launched.append(command))
    for name, value in {
        "pyInstallUpdateBrowserUrl": ASSET_URL,
        "pyInstallPlatform": "linux",
        "pyInstallInstalledPath": "/opt/webcontrol",
    }.items():
        monkeypatch.setattr(data, name, value)
    downloads = os.path.join(data.config.getHome(), ".WebControl", "downloads")
    yield SimpleNamespace(requested=requested, launched=launched, downloads=downloads)


def test_the_download_is_saved_under_its_asset_name(data, update):
    assert data.releaseManager.updatePyInstaller(bypassCheck=True) is True
    target = os.path.join(update.downloads, "webcontrol-0.99-linux-singlefile.tar.gz")
    with open(target, "rb") as handle:
        assert handle.read() == b"fake tarball"
    assert update.requested == [ASSET_URL]
    (command,) = update.launched
    assert command[1] == target


def test_an_asset_name_cannot_leave_the_downloads_directory(data, update, monkeypatch):
    monkeypatch.setattr(data, "pyInstallUpdateBrowserUrl", "https://example.invalid/x/..%2F..%2Fwebcontrol.json")
    data.releaseManager.updatePyInstaller(bypassCheck=True)
    (command,) = update.launched
    assert os.path.dirname(command[1]) == update.downloads


def test_a_failed_download_does_not_launch_the_upgrade(data, update, monkeypatch):
    def failing(url, **kwargs):
        def raise_for_status():
            raise release_module.requests.HTTPError("404")

        return SimpleNamespace(content=b"Not Found", status_code=404, raise_for_status=raise_for_status)

    monkeypatch.setattr(release_module.requests, "get", failing)
    assert data.releaseManager.updatePyInstaller(bypassCheck=True) is False
    assert update.launched == []


def test_a_stalled_download_gives_up_instead_of_hanging(data, update, monkeypatch):
    def stalls_without_timeout(url, **kwargs):
        if kwargs.get("timeout") is None:
            raise AssertionError("requests.get without a timeout waits forever on a stalled server")
        raise release_module.requests.Timeout("read timed out")

    monkeypatch.setattr(release_module.requests, "get", stalls_without_timeout)
    assert data.releaseManager.updatePyInstaller(bypassCheck=True) is False
    assert update.launched == []
