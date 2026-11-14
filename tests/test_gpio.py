"""GPIO buttons and LEDs, driven through gpiozero's MockFactory."""
import importlib

import gevent
import pytest
from gpiozero import BadPinFactory, Device
from gpiozero.pins.mock import MockFactory

import Actions.actions
from Actions import gpioActions
from helpers import decode_ui_message, drain


@pytest.fixture
def pin_factory():
    saved = Device.pin_factory
    yield
    Device.pin_factory = saved


@pytest.fixture
def gpio(data, pin_factory):
    """A GPIOActions on fresh mock pins, with the run state restored afterwards."""
    Device.pin_factory = MockFactory()
    saved = (data.gcode, data.gcodeIndex, data.manualZAxisAdjust, data.uploadFlag)
    actions = gpioActions.GPIOActions()
    actions.setUpData(data)
    drain(data.console_queue)
    yield actions
    for button in actions.Buttons:
        button.close()
    for _, led in actions.LEDs:
        led.close()
    data.gcode, data.gcodeIndex, data.manualZAxisAdjust, data.uploadFlag = saved


@pytest.fixture
def recorded_actions(data, monkeypatch):
    calls = []
    monkeypatch.setattr(data.actions, "processAction", calls.append)
    return calls


def press(pin_number):
    pin = Device.pin_factory.pin(pin_number)
    pin.drive_low()
    pin.drive_high()


def action(command):
    return {"data": {"command": command, "arg": "", "arg1": ""}}


@pytest.mark.parametrize(
    "setting, command",
    [("Stop", "stopRun"), ("Pause", "pauseRun"), ("Play", "startRun"), ("Go Home", "home"),
     ("Return to Center", "returnToCenter"), ("Set Home", "defineHome")],
)
def test_a_button_press_runs_its_action_through_process_action_on_the_hub(data, gpio, recorded_actions, setting, command):
    data.uploadFlag = 0
    gpio.setGPIOAction(17, setting)

    press(17)
    assert recorded_actions == []
    gevent.sleep(0)

    assert recorded_actions == [action(command)]


@pytest.mark.parametrize("flag, manual_z", [(2, False), (-1, False), (0, True)])
def test_play_resumes_a_paused_job(data, gpio, recorded_actions, flag, manual_z):
    data.uploadFlag, data.manualZAxisAdjust = flag, manual_z
    gpio.setGPIOAction(17, "Play")

    press(17)
    gevent.sleep(0)

    assert recorded_actions == [action("resumeRun")]


def test_play_while_a_job_runs_is_refused_like_the_ui(data, gpio):
    data.uploadFlag = 1
    data.gcode, data.gcodeIndex = ["G0 X1 ", "G1 Z-1 ", "G1 X2 "], 2
    gpio.setGPIOAction(17, "Play")
    drain(data.ui_queue1)

    press(17)
    gevent.sleep(0)

    assert drain(data.gcode_queue) == []
    assert [decode_ui_message(raw) for raw in drain(data.ui_queue1)] == [
        ("Alert", "Alert", "Cannot issue command while sending gcode.")
    ]


def test_buttons_debounce_their_pins(data, gpio):
    # MockPin stores bounce but does not simulate it, so this checks the setting reaches the pin.
    gpio.setGPIOAction(17, "Stop")

    assert Device.pin_factory.pin(17).bounce == 0.1


@pytest.mark.parametrize("setting", ["Spindle On", "Spindle Off", "Bogus"])
def test_an_unsupported_action_configures_nothing_and_says_so(data, gpio, setting):
    gpio.setGPIOAction(17, setting)

    assert gpio.Buttons == [] and gpio.LEDs == []
    assert drain(data.console_queue) == [f"GPIO pin 17: '{setting}' is not supported, pin left unconfigured"]


def test_status_leds_follow_the_run_state(data, gpio):
    gpio.setGPIOAction(22, "PlayLED")
    gpio.setGPIOAction(23, "StopLED")

    gpio.causeAction("PlayLED", "on")

    assert Device.pin_factory.pin(22).state and not Device.pin_factory.pin(23).state


def test_reassigning_a_pin_replaces_its_old_device(data, gpio, recorded_actions):
    data.uploadFlag = 0
    gpio.setGPIOAction(17, "Stop")
    gpio.setGPIOAction(17, "Pause")

    press(17)
    gevent.sleep(0)

    assert recorded_actions == [action("pauseRun")]
    assert len(gpio.Buttons) == 1


def test_importing_actions_leaves_the_pin_factory_alone(pin_factory):
    Device.pin_factory = None

    importlib.reload(Actions.actions)

    assert Device.pin_factory is None


def test_uses_the_real_pin_factory_when_one_loads(data, pin_factory, monkeypatch):
    real = object()
    monkeypatch.setattr(Device, "_default_pin_factory", staticmethod(lambda: real))
    drain(data.console_queue)

    gpioActions.selectPinFactory(data)

    assert Device.pin_factory is real
    assert drain(data.console_queue) == ["GPIO pin factory: object"]


def test_falls_back_to_mock_pins_without_real_gpio(data, pin_factory, monkeypatch):
    def unavailable():
        raise BadPinFactory("Unable to load any default pin factory!")

    monkeypatch.setattr(Device, "_default_pin_factory", staticmethod(unavailable))
    drain(data.console_queue)

    gpioActions.selectPinFactory(data)

    assert isinstance(Device.pin_factory, MockFactory)
    assert drain(data.console_queue) == [
        "GPIO pin factory: MockFactory (no GPIO available, pins are simulated: Unable to load any default pin factory!)"
    ]
