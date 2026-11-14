"""Where the bundled firmware lives, and how avrdude is invoked to flash it."""
import glob
import os
import re

# Must match the folders buildFirmware.sh writes to.
FIRMWARE_FOLDERS = {"custom": "madgrizzle", "stock": "maslowcnc", "holey": "holey"}

SERIAL_DEVICE = re.compile(
    r"/dev/(tty|cu)[A-Za-z0-9._-]*|/dev/serial/by-(id|path)/[A-Za-z0-9._:+-]+|COM[0-9]{1,3}"
)


def isSerialPortName(port, knownPorts=()):
    text = str(port)
    if ".." in text or text == "None":
        return False
    return text in knownPorts or SERIAL_DEVICE.fullmatch(text) is not None


def installRoot(data):
    return data.platformHome if data.platform == "PYINSTALLER" else "."


def firmwareFiles(root, variant):
    return sorted(glob.glob(os.path.join(root, "firmware", FIRMWARE_FOLDERS[variant], "*.hex")))


def avrdudeCommand(root, port, hexFile):
    tools = os.path.join(root, "tools")
    return [
        os.path.join(tools, "avrdude"),
        "-C" + os.path.join(tools, "avrdude.conf"),
        "-v",
        "-patmega2560",
        "-cwiring",
        "-P" + port,
        "-b115200",
        "-D",
        "-Uflash:w:" + hexFile + ":i",
    ]
