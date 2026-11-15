"""What WebControl can tell about the controller's firmware from the lines it prints (see spec/firmware.md)."""
import re

FIRMWARE_VERSION = re.compile(r"^Firmware Version\s+([\d.]+)")
BOARD_BANNER = "PCB v1."
# 51.29x prints its prescaler message with no newline, so a banner can arrive glued to other text.
BOOT_BANNER = re.compile(r"PCB v1\.|Grbl v1\.00|\bready\b")
# One boot prints three banner lines; a later banner within this many seconds is the same boot.
BOOT_BANNER_WINDOW = 10


def isBootBanner(line):
    return BOOT_BANNER.search(line) is not None


def boardBanner(line):
    return line[line.index(BOARD_BANNER):].strip() if BOARD_BANNER in line else None


def parseFirmwareVersion(line):
    match = FIRMWARE_VERSION.match(line)
    try:
        return float(match.group(1)) if match else None
    except ValueError:
        return None
