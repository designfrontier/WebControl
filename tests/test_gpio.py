"""GPIO buttons and LEDs, driven through gpiozero's MockFactory."""
import importlib

import pytest
from gpiozero import BadPinFactory, Device
from gpiozero.pins.mock import MockFactory

import Actions.actions
from Actions import gpioActions
from helpers import drain


@pytest.fixture
def pin_factory():
    saved = Device.pin_factory
    yield
    Device.pin_factory = saved


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
