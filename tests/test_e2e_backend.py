import cv2

import e2e_backend


def test_the_e2e_backend_never_opens_a_real_camera(monkeypatch):
    monkeypatch.setattr(cv2, "VideoCapture", cv2.VideoCapture)

    e2e_backend.disable_camera()

    stream = cv2.VideoCapture(0)
    assert cv2.VideoCapture is e2e_backend.NoCamera
    assert not stream.isOpened()
    assert stream.read() == (False, None)
