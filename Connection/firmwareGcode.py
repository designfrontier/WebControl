'''
How the Maslow firmware reads the words that pause it: M0/M1, and M6 with the tool chosen by T (spec/firmware.md §7).
'''
import re

PAUSE_WORD = re.compile(r"M0*[01](?![0-9.])")
TOOL_CHANGE_WORD = re.compile(r"M0*6(?![0-9.])")
TOOL_WORD = re.compile(r"T\s*([0-9]+(?:\.[0-9]*)?)")
FIRMWARE_NUMBER = re.compile(r"[+-]?([0-9]+\.?[0-9]*|\.[0-9]+)")
ONLY_TOOL_WORD = re.compile(r"^\s*(N[0-9]+\s*)?T\s*[0-9]+(\.[0-9]*)?\s*$")


def pausesFirmware(line):
    return PAUSE_WORD.search(line) is not None


def changesTool(line):
    return TOOL_CHANGE_WORD.search(line) is not None


def _isMachineCommand(line):
    return line.lstrip()[:1] in ("B", "$")


def onlySelectsTool(line):
    '''
    A line with nothing but a T word, which the firmware runs as an empty modal move.
    '''
    return ONLY_TOOL_WORD.match(line) is not None


def toolNamedBy(line):
    '''
    The tool the line selects as the file means it: its T word, wherever it stands.
    '''
    tool = None if _isMachineCommand(line) else TOOL_WORD.search(line)
    return None if tool is None else int(float(tool.group(1)))


def toolTheFirmwareReads(line):
    '''
    The firmware reads T only at the start of a line that also has a G or M word; it runs a lone `T2` as a move.
    '''
    text = line.strip().upper()
    firstGOrM = min((index for index in (text.find("G"), text.find("M")) if index != -1), default=-1)
    segment = text[:firstGOrM] if firstGOrM > 0 else ""
    if _isMachineCommand(text) or len(segment) < 2 or segment[0] != "T":
        return None
    number = FIRMWARE_NUMBER.match(segment[1:])
    return int(float(number.group(0))) if number else 0


def withToolFirst(line, tool):
    '''
    Rewrites an M6 line so the firmware reads `tool` from it, by leading the line with `T<tool>`.
    '''
    if _isMachineCommand(line) or not changesTool(line) or toolTheFirmwareReads(line) == tool:
        return line
    trailing = line[len(line.rstrip(" ")):]
    rest = " ".join(TOOL_WORD.sub("", line).split())
    return "T" + str(tool) + " " + rest + trailing


def toolChangeLine(line, nextTool, currentTool):
    '''
    :return: the line to send, the tool the firmware will have pending after it, and whether its M6 changes tool.
    '''
    named = toolNamedBy(line)
    nextTool = nextTool if named is None else named
    pauses = changesTool(line) and nextTool != currentTool
    # Even a same-tool M6 names the tool, so the firmware cannot pause on a T the host saw differently.
    return withToolFirst(line, nextTool), nextTool, pauses
