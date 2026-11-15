"""What WebControl can tell about the controller's firmware from the lines it prints (see spec/firmware.md)."""
import math
import re

FIRMWARE_VERSION = re.compile(r"^Firmware Version\s+([\d.]+)")
BOARD_BANNER = "PCB v1."
# 51.29x prints its prescaler message with no newline, so a banner can arrive glued to other text.
BOOT_BANNER = re.compile(r"PCB v1\.|Grbl v1\.00|\bready\b")
# One boot prints three banner lines; a later banner within this many seconds is the same boot.
BOOT_BANNER_WINDOW = 10


# Printed on every $ write by 51.292 but not 51.291; B05 reports both as 51.29.
EEPROM_REWRITE_NOTICE = "Change in setting, rewriting EEPROM"


def firmwareVersion(data):
    try:
        version = float(data.controllerFirmwareVersion)
    except (TypeError, ValueError):
        return 0.0
    return version if math.isfinite(version) else 0.0


def firmwareBuild(data):
    '''
    The reported version, with 51.29 narrowed to 51.292 when its notice was seen, else "51.29x"; None if unknown.
    '''
    version = firmwareVersion(data)
    if version <= 0:
        return None
    if math.isclose(version, 51.29):
        return "51.292" if data.firmwareRewriteNoticeSeen else "51.29x"
    return format(version, "g")


def supportsZaxisLimits(data):
    '''
    Only holey 51.292 and later store Z limits ($63/$64); B17-B19 do nothing or corrupt $43/$44 on every build.
    '''
    build = firmwareBuild(data)
    return build not in (None, "51.29x") and 51.292 <= float(build) < 100


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
