import copy
import time
from types import SimpleNamespace

import numpy
import pytest

import Actions.opticalCalibration as optical_module
from Actions.opticalCalibration import OpticalCalibration
from api.calibration import OPTICAL_DEFAULTS
from helpers import drain


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
    monkeypatch.setattr(optical_module, "time", SimpleNamespace(sleep=lambda seconds: None, time=time.time))
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


@pytest.fixture
def auto_home(optical, data, monkeypatch):
    """A 1x1 scan area starting in millimetres, with every setting a unit change touches restored afterwards."""
    monkeypatch.setattr(data.config, "settings", copy.deepcopy(data.config.settings))
    for key in ("tolerance", "gcodeShift", "feedRate", "measureRequest"):
        monkeypatch.setattr(data, key, getattr(data, key, None), raising=False)
    monkeypatch.setattr(data.actions, "updateSetting", data.actions.updateSetting)
    data.units = "MM"
    optical.tlX = optical.brX = optical.tlY = optical.brY = 0
    optical.camera = FakeCamera([square_frame()])
    return optical


def test_auto_home_remembers_the_units_it_started_in(auto_home, data):
    auto_home.on_AutoHome()
    assert auto_home.oldUnits == "MM"
    assert data.units == "INCHES"
    assert "G20 " in drain(data.gcode_queue)


def test_auto_home_puts_the_units_back_when_it_finishes(auto_home, data):
    auto_home.on_AutoHome()
    drain(data.gcode_queue)
    auto_home.on_AutoHome()
    assert auto_home.inAutoMode is False
    assert data.units == "MM"
    assert "G21 " in drain(data.gcode_queue)
    assert auto_home.oldUnits is None


def test_stopping_puts_the_units_back(auto_home, data):
    auto_home.on_AutoHome()
    assert auto_home.stopOpticalCalibration() is True
    assert data.units == "MM"
    assert drain(data.gcode_queue) == ["G21 "]


def test_auto_home_in_inches_leaves_the_units_alone(auto_home, data):
    data.units = "INCHES"
    auto_home.on_AutoHome()
    auto_home.on_AutoHome()
    assert data.units == "INCHES"
    assert "G21 " not in drain(data.gcode_queue)
