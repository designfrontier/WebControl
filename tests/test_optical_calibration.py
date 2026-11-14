import time

import numpy
import pytest

from Actions.opticalCalibration import OpticalCalibration
from api.calibration import OPTICAL_DEFAULTS


def square_frame(centre=(170, 125), size=60):
    image = numpy.full((240, 320, 3), 255, numpy.uint8)
    x, y = centre
    image[y - size // 2 : y + size // 2, x - size // 2 : x + size // 2] = 0
    return image


def blank_frame():
    return numpy.full((240, 320, 3), 255, numpy.uint8)


class FakeCamera:
    """read() returns the cached frame, as WebcamVideoStream does; only readNewFrame moves on."""

    def __init__(self, frames):
        self.frames = list(frames)
        self.reads = 0

    def read(self):
        return self.frames[0]

    def readNewFrame(self, previousCount, timeout=5.0):
        frame = self.frames[min(self.reads, len(self.frames) - 1)]
        self.reads += 1
        return frame, self.reads

    def start(self):
        return self

    def stop(self):
        pass


@pytest.fixture
def optical(data, monkeypatch):
    monkeypatch.setattr(time, "sleep", lambda seconds: None)
    optical = OpticalCalibration()
    optical.data = data
    optical.markerX = optical.markerY = 60 / 25.4
    optical.opticalCenter = (0, 0)
    optical.scaleX = optical.scaleY = 1.0
    return optical


def test_finds_the_square_with_opencv_4_contours(optical):
    optical.camera = FakeCamera([square_frame()] * 10)
    avgDx, avgDy, avgDi, avgxB, avgyB, xA, yA, image = optical.processImage(findCenter=True)
    assert (xA, yA) == (160, 120)
    assert avgxB == pytest.approx(169.5, abs=1)
    assert avgyB == pytest.approx(124.5, abs=1)
    assert avgDx == pytest.approx(9.5, abs=1)
    assert avgDy == pytest.approx(-4.5, abs=1)


def test_frames_without_contours_are_counted_as_bad(optical):
    optical.camera = FakeCamera([blank_frame()] * 10)
    assert optical.processImage(findCenter=True)[0] is None


def test_a_blank_frame_does_not_reuse_the_previous_square(optical):
    optical.camera = FakeCamera([square_frame(), blank_frame(), blank_frame()] * 10)
    assert optical.processImage(findCenter=True)[0] is None


def test_averages_ten_different_frames(optical):
    optical.camera = FakeCamera([square_frame(centre=(150 + 2 * n, 125)) for n in range(10)])
    avgxB = optical.processImage(findCenter=True)[3]
    assert optical.camera.reads == 10
    assert avgxB == pytest.approx(158.5, abs=1)


def test_a_camera_that_stops_delivering_frames_counts_as_bad(optical):
    optical.camera = FakeCamera([square_frame()])
    optical.camera.readNewFrame = lambda previousCount, timeout=5.0: (None, previousCount)
    assert optical.processImage(findCenter=True)[0] is None


def test_test_image_survives_a_camera_without_frames(optical, data, monkeypatch):
    monkeypatch.setattr(data, "opticalCalibrationTestImageUpdated", False, raising=False)
    optical.camera = FakeCamera([square_frame()])
    optical.camera.readNewFrame = lambda previousCount, timeout=5.0: (None, previousCount)
    assert optical.testImage(dict(OPTICAL_DEFAULTS)) is True
    assert data.opticalCalibrationTestImageUpdated is False
