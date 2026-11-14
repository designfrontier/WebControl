import time

import numpy
import pytest

from Actions.opticalCalibration import OpticalCalibration


def square_frame(centre=(340, 250), size=100):
    image = numpy.full((480, 640, 3), 255, numpy.uint8)
    x, y = centre
    image[y - size // 2 : y + size // 2, x - size // 2 : x + size // 2] = 0
    return image


def blank_frame():
    return numpy.full((480, 640, 3), 255, numpy.uint8)


class FakeCamera:
    def __init__(self, frames):
        self.frames = list(frames)
        self.reads = 0

    def read(self):
        frame = self.frames[min(self.reads, len(self.frames) - 1)]
        self.reads += 1
        return frame


@pytest.fixture
def optical(data, monkeypatch):
    monkeypatch.setattr(time, "sleep", lambda seconds: None)
    optical = OpticalCalibration()
    optical.data = data
    optical.markerX = optical.markerY = 100 / 25.4
    optical.opticalCenter = (0, 0)
    optical.scaleX = optical.scaleY = 1.0
    return optical


def test_finds_the_square_with_opencv_4_contours(optical):
    optical.camera = FakeCamera([square_frame()] * 10)
    avgDx, avgDy, avgDi, avgxB, avgyB, xA, yA, image = optical.processImage(findCenter=True)
    assert (xA, yA) == (320, 240)
    assert avgxB == pytest.approx(339.5, abs=1)
    assert avgyB == pytest.approx(249.5, abs=1)
    assert avgDx == pytest.approx(19.5, abs=1)
    assert avgDy == pytest.approx(-9.5, abs=1)


def test_frames_without_contours_are_counted_as_bad(optical):
    optical.camera = FakeCamera([blank_frame()] * 10)
    assert optical.processImage(findCenter=True)[0] is None


def test_a_blank_frame_does_not_reuse_the_previous_square(optical):
    optical.camera = FakeCamera([square_frame(), blank_frame(), blank_frame()] * 10)
    assert optical.processImage(findCenter=True)[0] is None
