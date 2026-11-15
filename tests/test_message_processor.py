"""Runs a MessageProcessor of its own on stub data, since the app's instance already consumes the real queues."""
import queue
import threading
import types

import pytest

from Background.messageProcessor import MessageProcessor

ALARM = "ALARM: The sled is not keeping up with its expected position and has halted.\r\n"


class StoppableQueue(queue.Queue):
    """Once stopped, raises SystemExit to end the processor's endless loop."""

    stopped = False

    def empty(self):
        if self.stopped:
            raise SystemExit
        return super().empty()


@pytest.fixture
def processor():
    release_check_done = threading.Event()
    data = types.SimpleNamespace(
        message_queue=StoppableQueue(),
        ui_controller_queue=queue.Queue(),
        console_queue=queue.Queue(),
        ui_queue1=types.SimpleNamespace(put=lambda *args: None),
        logger=types.SimpleNamespace(writeToLog=lambda message: None),
        releaseManager=types.SimpleNamespace(checkForLatestPyRelease=release_check_done.wait),
        helpManager=types.SimpleNamespace(checkForUpdatedHelp=lambda: None),
        lastChecked=float("inf"),
        uploadFlag=1,
        previousUploadStatus=0,
        version="test",
        stockFirmwareVersion=None,
        holeyFirmwareVersion=None,
        customFirmwareVersion=None,
    )
    message_processor = MessageProcessor()
    message_processor.setUpData(data)

    def run_until_stopped():
        try:
            message_processor.start()
        except SystemExit:
            pass

    thread = threading.Thread(target=run_until_stopped, daemon=True)
    yield types.SimpleNamespace(data=data, start=thread.start)
    data.message_queue.stopped = True
    release_check_done.set()
    thread.join(timeout=2)


def test_a_garbled_controller_line_does_not_stop_alarms_halting_the_job(processor):
    data = processor.data
    [data.message_queue.put(line) for line in ["[Measure:12.]3\r\n", "Firmware Version 1.2x\r\n", ALARM]]

    processor.start()

    assert data.ui_controller_queue.get(timeout=2) == "Firmware Version 1.2x\r\n"
    assert data.ui_controller_queue.get(timeout=2) == ALARM
    assert data.uploadFlag == 0


def test_a_slow_release_check_does_not_hold_up_alarms(processor):
    data = processor.data
    data.lastChecked = -1
    data.message_queue.put(ALARM)

    processor.start()

    assert data.ui_controller_queue.get(timeout=2) == ALARM
    assert data.uploadFlag == 0


def test_a_buffer_overflow_halts_the_job_like_an_alarm(processor):
    data = processor.data
    data.message_queue.put("Buffer overflow!\r\n")

    processor.start()

    assert data.ui_controller_queue.get(timeout=2) == "Buffer overflow!\r\n"
    assert (data.uploadFlag, data.previousUploadStatus) == (0, 1)
