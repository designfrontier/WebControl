"""Splits a WebControl v0.94 log.txt (and optionally alog.txt) into serial recordings, one per controller connect.

    .venv/bin/python tools/import_v094_log.py ~/.WebControl/log.txt --alog ~/.WebControl/alog.txt --out recordings/
"""
import argparse
import datetime
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
HEADER = "+++++"
ENTRY = re.compile(r"^(\d+\.\d+): (.*)$")
SENT = "Sent: "
SEND_FAILED = "Send FAILED: "
SUSPENDED = "Logging suspended due to user idle time"
RESUMED = "Logging resumed due to user activity"
NOT_SERIAL = ("Ground Control Version ",)
BOOT_BANNER = re.compile(r"PCB v1\.|Grbl v1\.00|\bready\b")
SETTING = re.compile(r"^\$(\d+)=\s*([-+0-9.eE]+)")
# v0.94 queues B05, the units and $$ when it opens the port; the banner the reset prints comes just before.
CONNECT_WRITE = "B05"
BANNER_LOOKBACK = 10.0
KEPT_SECTIONS = ("Maslow Settings", "Advanced Settings")


@dataclass
class Entry:
    t: float
    message: str


@dataclass
class Process:
    started: datetime.datetime
    entries: list = field(default_factory=list)
    line: int = 0


def parse(path, warn=print):
    '''
    :return: one Process per "+++++ / date / +++++" header, each with its timestamped entries.
    '''
    processes = []
    lines = Path(path).read_text(errors="replace").splitlines()
    index = 0
    while index < len(lines):
        text = lines[index]
        if text == HEADER and index + 2 < len(lines) and lines[index + 2] == HEADER:
            started = datetime.datetime.strptime(lines[index + 1].strip(), "%Y-%m-%d %H:%M:%S")
            processes.append(Process(started, line=index + 1))
            index += 3
            continue
        match = ENTRY.match(text)
        if match and processes:
            processes[-1].entries.append(Entry(float(match.group(1)), match.group(2)))
        elif text.strip():
            warn("%s:%d: not a log entry: %r" % (path, index + 1, text[:80]))
        index += 1
    return processes


def suspended_windows(entries):
    windows, start = [], None
    for entry in entries:
        if entry.message.startswith(SUSPENDED):
            start = entry.t
        elif entry.message.startswith(RESUMED) and start is not None:
            windows.append((start, entry.t))
            start = None
    return windows + ([(start, float("inf"))] if start is not None else [])


def merge_alog(process, alog_process):
    '''
    Fills the windows log.txt skipped while logging was suspended with alog.txt's entries (which omit status lines).
    '''
    windows = suspended_windows(process.entries)
    if not windows or alog_process is None:
        return process.entries
    filled = [entry for entry in alog_process.entries if any(start < entry.t <= end for start, end in windows)]
    return sorted(process.entries + filled, key=lambda entry: entry.t)


def is_marker(message):
    return message.startswith((SUSPENDED, RESUMED) + NOT_SERIAL)


def session_starts(entries):
    '''
    Indexes where a connect begins: the boot banner just before each B05 write, else the B05 itself.
    '''
    starts = []
    for index, entry in enumerate(entries):
        if entry.message.startswith(SENT) and entry.message[len(SENT):].startswith(CONNECT_WRITE):
            banner = [n for n in range(index - 1, -1, -1)
                      if entry.t - entries[n].t <= BANNER_LOOKBACK and BOOT_BANNER.search(entries[n].message)
                      and (not starts or n > starts[-1])]
            starts.append(min(banner) if banner else index)
    return starts


def records_for(entries, started, source, warn=print):
    origin = entries[0].t
    records = []
    for entry in entries:
        t = round(entry.t - origin, 2)
        message = entry.message
        if is_marker(message):
            continue
        if message.startswith(SEND_FAILED):
            warn("%s: a write failed at t=%.2f and is left out: %r" % (source, entry.t, message))
        elif message.startswith(SENT):
            records.append({"type": "tx", "t": t, "data": message[len(SENT):] + "\n"})
        else:
            records.append({"type": "rx", "t": t, "data": message + "\r\n"})
    session = {"type": "session", "t": 0, "started": (started + datetime.timedelta(seconds=origin)).isoformat(),
               "port": None, "units": units_of(records), "home": None, "settings": settings_of(records),
               "webcontrol": "0.94-log", "source": source}
    return [session] + records


def units_of(records):
    first = next((record["data"] for record in records if record["type"] == "tx" and record["data"][:3] in ("G20", "G21")),
                 None)
    return None if first is None else "INCHES" if first.startswith("G20") else "MM"


def firmware_keys():
    defaults = json.loads((REPO / "defaultwebcontrol.json").read_text())
    return {option["firmwareKey"]: (section, option) for section in KEPT_SECTIONS for option in defaults[section]
            if "firmwareKey" in option and option.get("type") in ("string", "float", "int")}


def settings_of(records):
    '''
    WebControl's settings as the handshake shows them: the $$ reply, overridden by the corrections WebControl wrote.
    '''
    keys = firmware_keys()
    values = {}
    for record in records:
        match = SETTING.match(record["data"])
        if match and int(match.group(1)) in keys:
            values[int(match.group(1))] = number_text(match.group(2))
    settings = {}
    for key, value in values.items():
        section, option = keys[key]
        settings.setdefault(section, {})[option["key"]] = value
    return settings


def number_text(text):
    return ("%.8f" % float(text)).rstrip("0").rstrip(".") if "." in text else text


def sessions(log, alog=None, warn=print):
    '''
    :return: (name, records) per connect, in log order.
    '''
    alog_by_start = {process.started: process for process in (parse(alog, warn) if alog else [])}
    found = []
    for process in parse(log, warn):
        entries = merge_alog(process, alog_by_start.get(process.started))
        starts = session_starts(entries)
        if not starts:
            continue
        for number, (begin, end) in enumerate(zip(starts, starts[1:] + [len(entries)]), 1):
            chunk = entries[begin:end]
            stamp = (process.started + datetime.timedelta(seconds=chunk[0].t)).strftime("%Y%m%d-%H%M%S")
            source = "%s line %d, connect %d" % (Path(log).name, process.line, number)
            found.append((stamp, records_for(chunk, process.started, source, warn)))
    return found


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("log", help="v0.94 log.txt")
    parser.add_argument("--alog", help="v0.94 alog.txt, to fill the gaps log.txt has while logging was suspended")
    parser.add_argument("--out", required=True, help="directory for the recordings")
    args = parser.parse_args(argv)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    warn = lambda message: print("warning: " + message, file=sys.stderr)  # noqa: E731
    written = set()
    for stamp, records in sessions(args.log, args.alog, warn):
        name = next(stamp + suffix for suffix in [""] + ["-" + str(n) for n in range(2, 10000)]
                    if stamp + suffix not in written)
        written.add(name)
        path = out / (name + ".jsonl")
        path.write_text("".join(json.dumps(record) + "\n" for record in records))
        tx = sum(record["type"] == "tx" for record in records)
        rx = sum(record["type"] == "rx" for record in records)
        print("%s  %5d tx %6d rx  %s" % (path, tx, rx, records[0]["source"]))


if __name__ == "__main__":
    main()
