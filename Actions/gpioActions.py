import warnings

import gevent
from DataStructures.makesmithInitFuncs import MakesmithInitFuncs
from gpiozero import Button, Device, LED, PinFactoryFallback
from gpiozero.pins.mock import MockFactory

BOUNCE_TIME = 0.1

BUTTON_COMMANDS = {
    "Stop": "stopRun",
    "Pause": "pauseRun",
    "Play": "startRun",
    "Set Home": "defineHome",
    "Go Home": "home",
    "Return to Center": "returnToCenter",
    "Shutdown": "shutdown",
}

LED_ACTIONS = ["PlayLED", "PauseLED", "StopLED"]


def selectPinFactory(data):
    '''
    Uses the Pi's real GPIO (or GPIOZERO_PIN_FACTORY) when it loads, and simulated pins anywhere else.
    '''
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", PinFactoryFallback)
            Device.pin_factory = Device._default_pin_factory()
        data.console_queue.put("GPIO pin factory: " + type(Device.pin_factory).__name__)
    # A factory named by GPIOZERO_PIN_FACTORY raises its own errors rather than BadPinFactory.
    except Exception as e:
        Device.pin_factory = MockFactory()
        data.console_queue.put("GPIO pin factory: MockFactory (no GPIO available, pins are simulated: " + str(e) + ")")


class GPIOActions(MakesmithInitFuncs):
    def __init__(self):
        self.Buttons = []
        self.LEDs = []

    actionList = [
        "Spindle On",
        "Spindle Off",
        "Shutdown",
        "Stop",
        "Pause",
        "Play",
        "Set Home",
        "Go Home",
        "Return to Center",
        "PlayLED",
        "PauseLED",
        "StopLED",
    ]

    def getActionList(self):
        return self.actionList

    def setup(self):
        selectPinFactory(self.data)
        setValues = self.data.config.getJSONSettingSection("GPIO Settings")
        for setting in setValues:
            if setting["value"] != "":
                pinNumber = int(setting["key"][4:])
                self.setGPIOAction(pinNumber, setting["value"])

    def setGPIOAction(self, pin, action):
        self.releasePin(pin)
        if action in BUTTON_COMMANDS:
            button = Button(pin, bounce_time=BOUNCE_TIME)
            button.when_pressed = self.pressHandler(action)
            self.Buttons.append(button)
            print(f"set Button with action: {action}")
        elif action in LED_ACTIONS:
            self.LEDs.append((action, LED(pin)))
            print(f"set LED with action: {action}")
        else:
            self.data.console_queue.put(f"GPIO pin {pin}: '{action}' is not supported, pin left unconfigured")

    def releasePin(self, pin):
        for button in [button for button in self.Buttons if button.pin.number == pin]:
            button.close()
            self.Buttons.remove(button)
        for led in [led for led in self.LEDs if led[1].pin.number == pin]:
            led[1].close()
            self.LEDs.remove(led)

    def pressHandler(self, action):
        # gpiozero calls this on its own native thread, so the action is handed to the gevent hub it was set up on.
        loop = gevent.get_hub().loop
        return lambda: loop.run_callback_threadsafe(self.runButtonAction, action)

    def runButtonAction(self, action):
        command = BUTTON_COMMANDS[action]
        if action == "Play" and (self.data.uploadFlag in (2, -1) or self.data.manualZAxisAdjust):
            command = "resumeRun"
        self.data.actions.processAction({"data": {"command": command, "arg": "", "arg1": ""}})

    def causeAction(self, action, onoff):
        for led in self.LEDs:
            if led[0] == action:
                if onoff == "on":
                    led[1].on()
                else:
                    led[1].off()
        if action == "PlayLED" and onoff == "on":
            self.causeAction("PauseLED", "off")
            self.causeAction("StopLED", "off")
        if action == "PauseLED" and onoff == "on":
            self.causeAction("PlayLED", "off")
            self.causeAction("StopLED", "off")
        if action == "StopLED" and onoff == "on":
            self.causeAction("PauseLED", "off")
            self.causeAction("PlayLED", "off")
