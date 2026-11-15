"""Splits a WebControl v0.94 log.txt (and optionally alog.txt) into serial recordings, one per controller connect.

    .venv/bin/python tools/import_v094_log.py ~/.WebControl/log.txt --alog ~/.WebControl/alog.txt --out recordings/
"""
import argparse
import datetime
import itertools
import json
import re
import sys
from collections import deque
from dataclasses import dataclass
from pathlib import Path
from typing import NamedTuple

REPO = Path(__file__).resolve().parent.parent
HEADER = "+++++"
DATE = re.compile(r"^\s*(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d)\s*$")
ENTRY = re.compile(r"^(\d+\.\d+): (.*)$")
SENT = "Sent: "
SEND_FAILED = "Send FAILED: "
SUSPENDED = "Logging suspended due to user idle time"
RESUMED = "Logging resumed due to user activity"
# WebControl's own message_queue puts (calibration and import errors) are logged as if the controller sent them.
NOT_SERIAL = ("Ground Control Version ", "Message: Please enter a number for ", "Message: The measurement between ",
              "Message: The bit diameter value of ", "Message: Cannot open import file.")
MULTI_LINE = "Message: "
NOT_IN_ALOG = ("<", "[", SUSPENDED, RESUMED)
BOOT_BANNER = re.compile(r"PCB v1\.|Grbl v1\.00|\bready\b")
SETTING = re.compile(r"^\$(\d+)=\s*([-+0-9.eE]+)")
# v0.94 queues B05, the units and $$ when it opens the port; the banner the reset prints comes just before.
CONNECT_WRITE = SENT + "B05"
BANNER_LOOKBACK = 10.0
HANDSHAKE_SECONDS = 10.0
HANDSHAKE_RECORDS = 1000
MAX_RECORDS = 500000
SPLIT_IDLE = 1800.0
KEPT_SECTIONS = ("Maslow Settings", "Advanced Settings")


@dataclass(frozen=True, eq=False)
class Header:
    started: datetime.datetime
    line: int


class Entry(NamedTuple):
    t: float
    message: str
    where: str


class Peekable:
    '''
    An iterator that reads ahead only when peeked, so taking a process does not drain its entries. None marks the end.
    '''
    UNREAD = object()

    def __init__(self, iterable):
        self.iterator = iter(iterable)
        self.head = Peekable.UNREAD

    def peek(self):
        if self.head is Peekable.UNREAD:
            self.head = next(self.iterator, None)
        return self.head

    def __iter__(self):
        return self

    def __next__(self):
        head = self.peek()
        if head is None:
            raise StopIteration
        self.head = Peekable.UNREAD
        return head


def lines_of(path, warn):
    '''
    (line number, text) split on \\n only; v0.94 keeps \\r and other control characters inside an entry.
    '''
    with open(path, "rb") as file:
        for number, raw in enumerate(file, 1):
            if not raw.endswith(b"\n"):
                warn("%s:%d: the last line is cut off and is left out: %r" % (path, number, raw[:80]))
                return
            yield number, raw.rstrip(b"\r\n").decode("utf-8", errors="replace")


def is_header(window):
    return (len(window) == 3 and window[0][1].endswith(HEADER) and DATE.match(window[1][1]) is not None
            and window[2][1] == HEADER)


def items(path, warn):
    '''
    Headers and entries in file order. A header glued onto a line a crash cut off still starts a process.
    '''
    name = Path(path).name
    lines = lines_of(path, warn)
    window = deque(itertools.islice(lines, 3))
    pending = None
    while window:
        number, text = window[0]
        if is_header(window):
            if text != HEADER:
                warn("%s:%d: a crash cut this line off before the next start: %r" % (path, number, text[:80]))
            if pending is not None:
                yield pending
            pending = None
            yield Header(datetime.datetime.strptime(DATE.match(window[1][1]).group(1), "%Y-%m-%d %H:%M:%S"), number)
            window.clear()
        else:
            window.popleft()
            match = ENTRY.match(text)
            if match:
                if pending is not None:
                    yield pending
                pending = Entry(float(match.group(1)), match.group(2), "%s:%d" % (name, number))
            elif pending is not None and pending.message.startswith(MULTI_LINE) and "\x00" not in text:
                pending = pending._replace(message=pending.message + "\n" + text)
            elif text.strip():
                warn("%s:%d: not a log entry: %r" % (path, number, text[:80]))
        window.extend(itertools.islice(lines, 3 - len(window)))
    if pending is not None:
        yield pending


def processes(path, warn):
    '''
    (Header, its entries) per WebControl start. Each process's entries must be read before the next is asked for.
    '''
    stream = Peekable(items(path, warn))
    while isinstance(stream.peek(), Entry):
        warn("%s: an entry before any +++++ header is left out" % next(stream).where)

    def entries():
        while isinstance(stream.peek(), Entry):
            yield next(stream)

    while stream.peek() is not None:
        header = next(stream)
        rest = entries()
        yield header, rest
        deque(rest, maxlen=0)


def in_alog(entry):
    return not entry.message.startswith(NOT_IN_ALOG)


def merge_alog(entries, alog_entries):
    '''
    log.txt's entries plus the ones only alog.txt has: those written while logging was suspended, or lost to a race.
    alog.txt holds every log.txt entry but status lines and markers, in the same order.
    '''
    alog = Peekable(alog_entries)
    for entry in entries:
        shared = in_alog(entry)
        while alog.peek() is not None and (alog.peek().t < entry.t or shared and alog.peek().t == entry.t):
            other = next(alog)
            if shared and (other.t, other.message) == (entry.t, entry.message):
                break
            yield other
        yield entry
    yield from alog


def with_alog(log_processes, alog_processes, warn):
    '''
    Pairs each log.txt process with alog.txt's in file order, skipping a start one file lacks.
    '''
    alog = Peekable(alog_processes)
    for header, entries in log_processes:
        while alog.peek() is not None and alog.peek()[0].started < header.started:
            warn("alog.txt line %d: no log.txt start matches it, left out" % next(alog)[0].line)
        if alog.peek() is not None and alog.peek()[0].started == header.started:
            yield header, merge_alog(entries, next(alog)[1])
        else:
            warn("log.txt line %d: alog.txt has no matching start" % header.line)
            yield header, entries


def monotonic(entries, warn):
    '''
    Shifts everything after a step back in time (racing flush threads, a clock set by NTP) so t never decreases.
    '''
    shift, last = 0.0, 0.0
    for entry in entries:
        t = entry.t + shift
        if t < last:
            warn("%s: time goes back %.2f s; later entries are shifted forward by that" % (entry.where, last - t))
            shift, t = shift + last - t, last
        last = t
        yield entry._replace(t=t) if shift else entry


def is_marker(message):
    return message.startswith((SUSPENDED, RESUMED) + NOT_SERIAL)


def numbered_connects(entries):
    '''
    (connect number, entry) from each connect's start: the boot banner up to 10 s before a B05 write, else the B05.
    Entries before a process's first connect are dropped.
    '''
    number, recent = 0, deque()
    for entry in entries:
        while recent and entry.t - recent[0].t > BANNER_LOOKBACK:
            old = recent.popleft()
            if number:
                yield number, old
        if not entry.message.startswith(CONNECT_WRITE):
            recent.append(entry)
            continue
        banner = next((index for index, other in enumerate(recent) if BOOT_BANNER.search(other.message)), len(recent))
        before = [(number, other) for other in itertools.islice(recent, banner)] if number else []
        number += 1
        yield from before + [(number, other) for other in itertools.islice(recent, banner, None)] + [(number, entry)]
        recent.clear()
    yield from ((number, other) for other in recent) if number else ()


def record_of(entry, source, warn):
    message = entry.message
    if message.startswith(SEND_FAILED):
        warn("%s: a write failed at t=%.2f and is left out: %r" % (source, entry.t, message))
        return None
    if message.startswith(SENT):
        return {"type": "tx", "t": entry.t, "data": message[len(SENT):] + "\n"}
    return {"type": "rx", "t": entry.t, "data": message + "\r\n"}


def connect_records(entries, started, source, warn, max_records=MAX_RECORDS, split_idle=SPLIT_IDLE):
    '''
    A connect's records, after a session line per part: a new part starts after max_records or split_idle s without tx.
    Units and settings come from the handshake, the first HANDSHAKE_SECONDS.
    '''
    pairs = Peekable((entry, record) for entry in entries for record in [record_of(entry, source, warn)] if record)
    if pairs.peek() is None:
        return
    origin = pairs.peek()[0].t
    handshake = []
    while (pairs.peek() is not None and pairs.peek()[0].t - origin <= HANDSHAKE_SECONDS
           and len(handshake) < HANDSHAKE_RECORDS):
        handshake.append(next(pairs))
    head = [record for _, record in handshake]
    found = {"units": units_of(head), "settings": settings_of(head)}
    part, count, last_tx = 0, 0, None
    for entry, record in itertools.chain(handshake, pairs):
        tx = record["type"] == "tx"
        if part == 0 or count >= max_records or (tx and last_tx is not None and entry.t - last_tx >= split_idle):
            part, count, origin = part + 1, 0, entry.t
            yield {"type": "session", "t": 0, "started": (started + datetime.timedelta(seconds=origin)).isoformat(),
                   "port": None, "units": found["units"], "home": None, "settings": found["settings"],
                   "webcontrol": "0.94-log", "source": source + (", part %d" % part if part > 1 else "")}
        count += 1
        last_tx = entry.t if tx else last_tx
        yield {**record, "t": round(entry.t - origin, 2)}


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


def recordings(log, alog=None, warn=print, max_records=MAX_RECORDS, split_idle=SPLIT_IDLE):
    '''
    Every record of every connect in log order, lazily; a session record starts each recording.
    '''
    log_processes = processes(log, warn)
    paired = with_alog(log_processes, processes(alog, warn), warn) if alog else log_processes
    for header, entries in paired:
        serial = (entry for entry in monotonic(entries, warn) if not is_marker(entry.message))
        for number, connect in itertools.groupby(numbered_connects(serial), key=lambda pair: pair[0]):
            source = "%s line %d, connect %d" % (Path(log).name, header.line, number)
            yield from connect_records((entry for _, entry in connect), header.started, source, warn, max_records,
                                       split_idle)


def stamp_of(session):
    return datetime.datetime.fromisoformat(session["started"]).strftime("%Y%m%d-%H%M%S")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("log", help="v0.94 log.txt")
    parser.add_argument("--alog", help="v0.94 alog.txt, to fill the gaps log.txt has while logging was suspended")
    parser.add_argument("--out", required=True, help="directory for the recordings")
    parser.add_argument("--max-records", type=int, default=MAX_RECORDS, help="start a new file after this many")
    parser.add_argument("--split-idle", type=float, default=SPLIT_IDLE,
                        help="start a new file at a write after this many seconds without one")
    args = parser.parse_args(argv)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    warn = lambda message: print("warning: " + message, file=sys.stderr)  # noqa: E731
    written, file, counts = set(), None, None

    def close():
        if file is not None:
            file.close()
            print("%s  %5d tx %6d rx  %s" % (file.name, counts["tx"], counts["rx"], counts["source"]))

    for record in recordings(args.log, args.alog, warn, args.max_records, args.split_idle):
        if record["type"] == "session":
            close()
            stamp = stamp_of(record)
            name = next(stamp + suffix for suffix in itertools.chain([""], ("-%d" % n for n in itertools.count(2)))
                        if stamp + suffix not in written)
            written.add(name)
            file = open(out / (name + ".jsonl"), "w", encoding="utf-8")
            counts = {"tx": 0, "rx": 0, "source": record["source"]}
        else:
            counts[record["type"]] += 1
        file.write(json.dumps(record) + "\n")
    close()


if __name__ == "__main__":
    main()
