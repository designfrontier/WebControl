"""
Opt-in capture of every serial line, for turning real jobs into contract/serial transcripts.

Records go through a bounded queue to a writer thread, so the serial thread never waits on the disk; when the queue is
full a record is dropped and counted. Format: contract/serial/recording.schema.json.
"""
import datetime
import json
import os
import queue
import threading
import time
from pathlib import Path

QUEUE_LIMIT = 20000
MAX_FILE_BYTES = 5 * 1024 * 1024
MAX_FILES = 40


def defaultDirectory():
    return Path.home() / ".WebControl" / "serial-recordings"


class SerialRecorder:
    def __init__(self, directory=None, clock=time.monotonic, wallClock=datetime.datetime.now,
                 queueLimit=QUEUE_LIMIT, maxFileBytes=MAX_FILE_BYTES, maxFiles=MAX_FILES):
        self.directory = Path(directory) if directory is not None else defaultDirectory()
        self.clock = clock
        self.wallClock = wallClock
        self.maxFileBytes = maxFileBytes
        self.maxFiles = maxFiles
        self.queue = queue.Queue(queueLimit)
        self.enabled = False
        self.dropped = 0
        self.sessionStart = None
        self.seq = 0
        self.cause = None
        self.writer = None
        self.writerLock = threading.Lock()
        self.file = None
        self.sessionName = None
        self.part = 0
        self.droppedWritten = 0

    def setEnabled(self, enabled):
        self.enabled = bool(enabled)

    def startSession(self, port, units, home, settings, webcontrol):
        if not self.enabled:
            return
        self.sessionStart = self.clock()
        self.seq = 0
        self.cause = None
        self._put({"type": "session", "started": self.wallClock().isoformat(timespec="seconds"), "port": port,
                   "units": units, "home": home, "settings": settings, "webcontrol": webcontrol}, at=self.sessionStart)

    def tx(self, data):
        if self.enabled:
            self._put({"type": "tx", "data": data, **({} if self.cause is None else {"cause": self.cause})})

    def rx(self, data):
        if self.enabled:
            self._put({"type": "rx", "data": data})

    def action(self, command, arg, arg1):
        if not self.enabled:
            return
        self.seq += 1
        self.cause = self.seq
        self._put({"type": "ui", "seq": self.seq, "command": command, "arg": arg, "arg1": arg1})

    def gcodeLoaded(self, filename, lines):
        if self.enabled:
            self._put({"type": "gcode", "filename": filename, "lines": lines})

    def _put(self, record, at=None):
        try:
            now = self.clock() if at is None else at
            if self.sessionStart is None:
                self.sessionStart = now
            record["t"] = round(now - self.sessionStart, 6)
            self.queue.put_nowait(record)
            self._ensureWriter()
        except queue.Full:
            self.dropped += 1
        except Exception:  # recording must never break the caller
            pass

    def _ensureWriter(self):
        if self.writer is not None:
            return
        with self.writerLock:
            if self.writer is None:
                self.writer = threading.Thread(target=self._writeForever, daemon=True)
                self.writer.start()

    def _writeForever(self):
        while True:
            record = self.queue.get()
            try:
                self.write(record)
                if self.queue.empty() and self.file is not None:
                    self.file.flush()
            except Exception as error:
                print("serial recording failed: " + str(error))
            finally:
                self.queue.task_done()

    def write(self, record):
        '''
        Writes one record (and a note of any records dropped before it); runs on the writer thread.
        '''
        if record["type"] == "session" or self.file is None:
            self._openSession(record)
        if self.dropped > self.droppedWritten:
            self._writeLine({"type": "dropped", "count": self.dropped - self.droppedWritten, "t": record["t"]})
            self.droppedWritten = self.dropped
        self._writeLine(record)

    def _writeLine(self, record):
        if self.file.tell() >= self.maxFileBytes:
            self.part += 1
            self._open()
        self.file.write(json.dumps(record, ensure_ascii=False) + "\n")

    def _openSession(self, record):
        stamp = self.wallClock().strftime("%Y%m%d-%H%M%S")
        taken = {path.name.split(".")[0] for path in self.directory.glob("*.jsonl")} if self.directory.exists() else set()
        self.sessionName = next(name for name in (stamp + suffix for suffix in [""] + ["-" + str(n) for n in range(1, 1000)])
                                if name not in taken)
        self.part = 0
        self._open()

    def _open(self):
        if self.file is not None:
            self.file.close()
        self.directory.mkdir(parents=True, exist_ok=True)
        name = self.sessionName + ("" if self.part == 0 else "." + str(self.part)) + ".jsonl"
        self.file = open(self.directory / name, "a", encoding="utf-8")
        self._prune()

    def _prune(self):
        files = sorted(self.directory.glob("*.jsonl"), key=lambda path: (path.stat().st_mtime, path.name))
        [os.remove(path) for path in files[:max(0, len(files) - self.maxFiles)]]

    def flush(self, timeout=5.0):
        '''
        Waits until the writer has written everything queued so far; for tests and shutdown.
        '''
        deadline = time.time() + timeout
        while self.queue.unfinished_tasks and time.time() < deadline:
            time.sleep(0.01)
        if self.file is not None:
            self.file.flush()
