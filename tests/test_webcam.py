import base64
import time

import cv2
import gevent
import gevent.monkey
import numpy
import pytest

from Background.webcamVideoStream import WebcamVideoStream

real_sleep = gevent.monkey.get_original("time", "sleep")


class SlowCapture:
    """A camera whose read() blocks the calling OS thread, as cv2.VideoCapture.read() does."""

    def __init__(self, camera, reads_before_stop, delay=0.15):
        self.camera = camera
        self.reads_before_stop = reads_before_stop
        self.delay = delay
        self.reads = 0
        self.released = False

    def read(self):
        real_sleep(self.delay)
        self.reads += 1
        if self.reads >= self.reads_before_stop:
            self.camera.stopped = True
        return True, numpy.full((48, 64, 3), self.reads, numpy.uint8)

    def release(self):
        self.released = True


@pytest.fixture
def camera(data, monkeypatch):
    for key in ("cameraImage", "cameraImageUpdated", "continuousCamera"):
        monkeypatch.setattr(data, key, getattr(data, key, None))
    data.continuousCamera = False
    # main.py monkey-patches time in production; conftest turns that off, so put the cooperative sleep back.
    monkeypatch.setattr(time, "sleep", gevent.sleep)
    camera = WebcamVideoStream()
    camera.data = data
    camera.stopped = False
    return camera


def ticker(ticks):
    while True:
        ticks.append(time.monotonic())
        gevent.sleep(0.01)


def test_capture_and_encoding_do_not_block_other_greenlets(camera, data):
    camera.stream = SlowCapture(camera, reads_before_stop=3)
    ticks = []
    other = gevent.spawn(ticker, ticks)
    gevent.spawn(camera.update).join(timeout=5)
    other.kill()
    assert camera.stream is None
    assert len(ticks) >= 20


def test_capture_publishes_the_encoded_frame(camera, data):
    camera.stream = SlowCapture(camera, reads_before_stop=2, delay=0)
    camera._grab()
    image = cv2.imdecode(numpy.frombuffer(base64.b64decode(data.cameraImage), numpy.uint8), cv2.IMREAD_COLOR)
    assert image.shape == (192, 256, 3)
    assert data.cameraImageUpdated is True
    assert camera.frame.shape == (48, 64, 3)


def test_a_missing_frame_keeps_the_last_image(camera, data):
    class EmptyCapture:
        def read(self):
            return False, None

    data.cameraImage = "previous"
    data.cameraImageUpdated = False
    camera.stream = EmptyCapture()
    camera._grab()
    assert data.cameraImage == "previous"
    assert data.cameraImageUpdated is False
    assert camera.frame is None


def test_changing_the_sleep_interval_restarts_the_capture_thread(camera, monkeypatch):
    class Thread:
        joined = False

        def join(self):
            self.joined = True

    camera.stream = object()
    camera.th = Thread()
    started = []
    monkeypatch.setattr(camera, "start", lambda: started.append(camera.stopped))
    camera.changeSetting("cameraSleep", 0.05)
    assert camera.th.joined is True
    assert started == [True]
    assert camera.cameraSleep == 0.05
