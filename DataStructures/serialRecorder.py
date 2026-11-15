"""
Opt-in capture of every serial line, for turning real jobs into contract/serial transcripts.

Records go through a bounded queue to a writer on a real OS thread, so the serial loop never waits on the disk; when
the queue is full a record is dropped and counted. Format: contract/serial/recording.schema.json.
"""
import collections
import datetime
import json
import os
import time
from pathlib import Path

from gevent import monkey

# gevent's patched threads are greenlets on the serial loop's hub, where a blocking write would stall streaming.
startNativeThread = monkey.get_original("_thread", "start_new_thread")
nativeSleep = monkey.get_original("time", "sleep")

QUEUE_LIMIT = 20000
MAX_FILE_BYTES = 5 * 1024 * 1024
MAX_FILES = 40
WRITER_IDLE_SECONDS = 0.05


def defaultDirectory():
    return Path.home() / ".WebControl" / "serial-recordings"


class SerialRecorder:
    def __init__(self, directory=None, clock=time.monotonic, wallClock=datetime.datetime.now,
                 queueLimit=QUEUE_LIMIT, maxFileBytes=MAX_FILE_BYTES, maxFiles=MAX_FILES):
        self.directory = Path(directory) if directory is not None else defaultDirectory()
        self.clock = clock
        self.wallClock = wallClock
        self.queueLimit = queueLimit
        self.maxFileBytes = maxFileBytes
        self.maxFiles = maxFiles
        self.queue = collections.deque()
        self.enabled = False
        self.dropped = 0
        self.droppedSinceLastPut = 0
        self.sessionStart = None
        self.seq = 0
        self.cause = None
        self.writerRunning = False
        self.writerBusy = False
        self.failed = False
        self.file = None
        self.fileBytes = 0
        self.sessionName = None
        self.part = 0

    def setEnabled(self, enabled):
        self.enabled = bool(enabled)

    def startSession(self, port, units, home, settings, webcontrol, gcode=None):
        if not self.enabled:
            return
        self.sessionStart = self.clock()
        self.seq = 0
        self.cause = None
        self._put({"type": "session", "started": self.wallClock().isoformat(timespec="seconds"), "port": port,
                   "units": units, "home": home, "settings": settings, "webcontrol": webcontrol,
                   **({"gcode": gcode} if gcode else {})}, at=self.sessionStart)

    def tx(self, data):
        if self.enabled:
            self._put({"type": "tx", "data": data, **({} if self.cause is None else {"cause": self.cause})})

    def rx(self, data):
        if self.enabled:
            self._put({"type": "rx", "data": data})

    def action(self, command, arg, arg1):
        if not self.enabled or self.sessionStart is None:
            return
        self.seq += 1
        self.cause = self.seq
        self._put({"type": "ui", "seq": self.seq, "command": command, "arg": arg, "arg1": arg1})

    def gcodeLoaded(self, filename, lines):
        if self.enabled:
            self._put({"type": "gcode", "filename": filename, "lines": lines})

    def _put(self, record, at=None):
        try:
            if self.sessionStart is None:
                return
            now = self.clock() if at is None else at
            record["t"] = round(now - self.sessionStart, 6)
            if len(self.queue) >= self.queueLimit:
                self.dropped += 1
                self.droppedSinceLastPut += 1
                return
            if self.droppedSinceLastPut:
                self.queue.append({"type": "dropped", "count": self.droppedSinceLastPut, "t": record["t"]})
                self.droppedSinceLastPut = 0
            self.queue.append(record)
            self._ensureWriter()
        except Exception:  # recording must never break the caller
            pass

    def _ensureWriter(self):
        if self.writerRunning:
            return
        self.writerRunning = True
        try:
            startNativeThread(self._writeForever, ())
        except Exception:
            self.writerRunning = False

    def _writeForever(self):
        try:
            while True:
                self._drain()
                nativeSleep(WRITER_IDLE_SECONDS)
        finally:
            self.writerRunning = False

    def _drain(self):
        self.writerBusy = True
        try:
            while self.queue:
                self.writeSafely(self.queue.popleft())
            if self.file is not None and not self.failed:
                self.writeSafely(None)
        finally:
            self.writerBusy = False

    def writeSafely(self, record):
        '''
        Writes one record, or flushes for None; after an error everything is discarded until the next session.
        '''
        if self.failed and (record is None or record["type"] != "session"):
            return
        try:
            self.file.flush() if record is None else self.write(record)
            self.failed = False
        except Exception as error:
            self.failed = True
            self._closeQuietly()
            print("serial recording stopped until the next connect: " + str(error))

    def write(self, record):
        if record["type"] == "session" or self.file is None:
            self._openSession()
        self._writeLine(record)

    def _writeLine(self, record):
        if self.fileBytes >= self.maxFileBytes:
            self.part += 1
            self._open()
        line = (json.dumps(record, ensure_ascii=False) + "\n").encode("utf-8")
        self.file.write(line)
        self.fileBytes += len(line)

    def _openSession(self):
        stamp = self.wallClock().strftime("%Y%m%d-%H%M%S")
        taken = {path.name.split(".")[0] for path in self.directory.glob("*.jsonl")} if self.directory.exists() else set()
        self.sessionName = next(name for name in (stamp + suffix for suffix in [""] + ["-" + str(n) for n in range(1, 1000)])
                                if name not in taken)
        self.part = 0
        self._open()

    def _open(self):
        self._closeQuietly()
        self.directory.mkdir(parents=True, exist_ok=True)
        name = self.sessionName + ("" if self.part == 0 else "." + str(self.part)) + ".jsonl"
        self.file = open(self.directory / name, "ab")
        self.fileBytes = self.file.tell()
        self._prune()

    def _closeQuietly(self):
        file, self.file = self.file, None
        try:
            file is not None and file.close()
        except Exception:
            pass

    def _prune(self):
        sessionHeader = self.sessionName + ".jsonl"
        files = sorted((path for path in self.directory.glob("*.jsonl") if path.name != sessionHeader),
                       key=lambda path: (path.stat().st_mtime, path.name))
        [os.remove(path) for path in files[:max(0, len(files) - (self.maxFiles - 1))]]

    def flush(self, timeout=5.0):
        '''
        Waits until the writer has written everything queued so far; for tests and shutdown.
        '''
        deadline = time.time() + timeout
        while (self.queue or self.writerBusy) and time.time() < deadline:
            time.sleep(0.01)
        if self.file is not None and not self.failed:
            self.writeSafely(None)
