"""Boots main.py's app once per session with no serial port, camera, gevent patching or network checks."""
import atexit
import os
import shutil
import sys
import tempfile

import pytest

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Config reads ~/.WebControl at import time, so HOME must point somewhere disposable before anything imports it.
os.environ["HOME"] = tempfile.mkdtemp(prefix="webcontrol-pytest-")
atexit.register(shutil.rmtree, os.environ["HOME"], ignore_errors=True)
os.chdir(REPO)
sys.path.insert(0, REPO)

import gevent.monkey  # noqa: E402

gevent.monkey.patch_all = lambda *args, **kwargs: None

from Background.webcamVideoStream import WebcamVideoStream  # noqa: E402
from Connection.serialPort import SerialPort  # noqa: E402
from DataStructures.data import Data  # noqa: E402

serial_open_calls = []
SerialPort.openConnection = lambda self, *args: serial_open_calls.append(args)
WebcamVideoStream.getSettings = lambda self, src=0: None
# The hourly GitHub release check fires when lastChecked is over an hour old.
Data.lastChecked = float("inf")

import schedule  # noqa: E402
from helpers import NAMESPACE, drain  # noqa: E402

import main  # noqa: E402

schedule.clear()
# A truthy uithread stops the connect handler from starting UIProcessor's endless loop.
main.app.uithread = "disabled-in-tests"


@pytest.fixture(scope="session")
def app():
    main.app.config["TESTING"] = True
    return main.app


@pytest.fixture
def data(app):
    """The live Data object, with queues drained and run state reset around each test."""
    data = app.data
    saved = {"uploadFlag": data.uploadFlag, "jobInProgress": data.jobInProgress, "xval": data.xval, "yval": data.yval, "units": data.units,
             "probeInFlight": data.probeInFlight}
    queues = [data.gcode_queue, data.ui_queue1, data.quick_queue]
    for queue in queues:
        drain(queue)
    yield data
    for queue in queues:
        drain(queue)
    for key, value in saved.items():
        setattr(data, key, value)


@pytest.fixture
def client(app):
    return app.test_client()


@pytest.fixture
def socket_client(app):
    test_client = main.socketio.test_client(app, namespace=NAMESPACE)
    yield test_client
    if test_client.is_connected(NAMESPACE):
        test_client.disconnect(NAMESPACE)


@pytest.fixture
def connect_socket(app):
    """Factory for extra socket clients, all disconnected at teardown."""
    clients = []

    def connect():
        test_client = main.socketio.test_client(app, namespace=NAMESPACE)
        clients.append(test_client)
        return test_client

    yield connect
    for test_client in clients:
        if test_client.is_connected(NAMESPACE):
            test_client.disconnect(NAMESPACE)
