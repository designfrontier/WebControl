import warnings

from DataStructures.makesmithInitFuncs import MakesmithInitFuncs
from gpiozero import Button, Device, LED, PinFactoryFallback
from gpiozero.pins.mock import MockFactory


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
        pass

    """
    get gpio settings
    """
    Buttons = []
    LEDs = []
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
        # print(setValues)
        for setting in setValues:
            if setting["value"] != "":
                pinNumber = int(setting["key"][4:])
                self.setGPIOAction(pinNumber, setting["value"])

    def setGPIOAction(self, pin, action):
        # first remove pin assignments if already made
        foundButton = None
        for button in self.Buttons:
            if button.pin.number == pin:
                button.pin.close()
                foundButton = button
                break
        if foundButton is not None:
            self.Buttons.remove(foundButton)

        foundLED = None
        for led in self.LEDs:
            if led[1].pin.number == pin:
                led[1].pin.close()
                foundLED = led
                break
        if foundLED is not None:
            self.LEDs.remove(foundLED)

        type, pinAction = self.getAction(action)
        if type == "button":
            button = Button(pin)
            button.when_pressed = pinAction
            self.Buttons.append(button)
            print(f"set Button with action: {action}")
        if type == "led":
            _led = LED(pin)
            led = (action, _led)
            self.LEDs.append(led)
            print(f"set LED with action: {action}")

    def getAction(self, action):
        if action == "Stop":
            return "button", self.data.actions.stopRun
        elif action == "Pause":
            return "button", self.data.actions.pauseRun
        elif action == "Play":
            return "button", self.data.actions.startRun
        else:
            return "led", None

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
