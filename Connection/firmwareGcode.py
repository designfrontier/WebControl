'''
How the Maslow firmware reads the words that pause it: M0/M1, and M6 with the tool chosen by T (spec/firmware.md §7).
'''
import re

PAUSE_WORD = re.compile(r"M0*[01](?![0-9.])")
TOOL_CHANGE_WORD = re.compile(r"M0*6(?![0-9.])")
G_OR_M_WORD = re.compile(r"[GM]\s*[0-9]")
TOOL_WORD = re.compile(r"T\s*([0-9]+)(?![0-9.])")


def pausesFirmware(line):
    return PAUSE_WORD.search(line) is not None


def changesTool(line):
    return TOOL_CHANGE_WORD.search(line) is not None


def _isMachineCommand(line):
    return line.lstrip()[:1] in ("B", "$")


def toolTheFirmwareReads(line):
    '''
    The firmware takes T only before the line's first G or M word, so `M6 T2` keeps the current tool.
    '''
    if _isMachineCommand(line):
        return None
    firstGOrM = G_OR_M_WORD.search(line)
    tool = TOOL_WORD.search(line[:firstGOrM.start()] if firstGOrM else line)
    return None if tool is None else int(tool.group(1))


def withToolWordFirst(line):
    '''
    Moves a T that follows the G/M words of an M6 line to the front, so the firmware changes to the tool the line names.
    '''
    if _isMachineCommand(line) or not changesTool(line) or toolTheFirmwareReads(line) is not None:
        return line
    tool = TOOL_WORD.search(line)
    if tool is None:
        return line
    rest = (line[:tool.start()].rstrip(" ") + " " + line[tool.end():].lstrip(" ")).strip(" ")
    trailing = line[len(line.rstrip(" ")):]
    return tool.group(0).replace(" ", "") + " " + rest + trailing
