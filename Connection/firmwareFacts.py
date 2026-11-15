"""What WebControl can tell about the controller's firmware from the lines it prints (see spec/firmware.md)."""
import re

FIRMWARE_VERSION = re.compile(r"^Firmware Version\s+([\d.]+)")


def parseFirmwareVersion(line):
    match = FIRMWARE_VERSION.match(line)
    try:
        return float(match.group(1)) if match else None
    except ValueError:
        return None
