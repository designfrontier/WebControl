---
layout: default
title: Job Queue
nav_order: 5
---
# Job Queue

The job queue runs gcode files one after another: when one file ends, the next one starts on its own.

## Adding files

- **G-Code ▸ Open**: choose a file and press **Add to queue**. The file is added to the end of the queue without
  being opened, so you can add files while a job runs. Tick **Don't stop for bit changes in this file** first to add
  it with its override on (below).
- **G-Code ▸ Job Queue ▸ Add loaded file** adds the file that is loaded now.

The queue is kept on the Raspberry Pi, so every browser sees the same queue, and it is still there after you reload
the page or restart WebControl. After a restart it never starts by itself: press Play or **Start queue**.

## Running the queue

The first file in the queue is the one that runs. Start the queue in either of two ways:

- Load the first file and press **Play**, as for any job.
- Press **Start queue** (in G-Code ▸ Job Queue). This loads the first file, raises Z and checks the bit as between
  files (below), then runs the file from its first line.

When a file ends, and the controller has finished its last line, WebControl:

1. Takes the finished file off the queue and loads the next one.
2. Raises Z to the safe height (Settings ▸ Maslow ▸ Z-Axis Safe Travel Height), or keeps the bit higher if it is
   already higher.
3. Checks the bit. The bit in the spindle is the last tool the files before named, or the one you last confirmed at a
   tool-change prompt with **Resume**.
   - **The next file names another tool than that bit**: the spindle is turned off (`M5`) and the usual tool-change
     prompt asks you to insert the tool. Change the bit, set Z, and press **Resume**. Z rises again and the file
     starts.
   - **Otherwise the file starts at once**: it names the same tool, it names no tool, or no file or tool change so
     far says which bit is in. A tool change inside the file still pauses as in any job.

The queue line above the Play button shows what runs, what comes next and what happens between the files, for
example "Next: shelf.nc · same tool — starts immediately" or "Next: sign.nc · Tool 2 (6 mm flat end mill) — will
pause for a bit change". A tool's description comes from Settings ▸ Tools.

**Pause** and **Resume** work as for any job, and the queue goes on when the file ends.

With a park position set ([Parking](parking.md)), the sled parks after the queue's last file, never between files.

## Skipping bit changes in a file

A file added with **Don't stop for bit changes** (or with the box ticked in G-Code ▸ Job Queue) never waits for a bit
change when the queue runs it: not between files, and not at its own tool changes. Its tool-change lines are sent
without their `T` and `M6` words, so the controller does not stop either, and the cut goes on with the bit that is
in. The file shows **bit changes skipped**, and the server log records each skipped change with its line. The queue
still follows the tools the file names, so the file after it is compared with the file's last tool.

Use it only when the bit in the spindle is right for every tool the file names. The override applies only to queue
runs: Play of the same file outside the queue pauses at its tool changes as usual. It cannot be changed on the file
that is running.

## When the queue stops

The queue does not start the next file after any of these. The files still in the queue stay there, and the queue
line says why it stopped:

- you pressed **Stop**;
- the controller raised an alarm;
- the USB connection to the controller was lost, or the controller restarted;
- a line of the file could not be sent, or did not reach the controller;
- Touch Z in the file failed;
- the bit's height could not be found, or the safe height is 0 or less;
- the next file could not be loaded.

Fix the cause, then continue:

- If **Stop** interrupted the first file part way, the queue line offers **From line N (where it stopped)**, which
  returns to that line as starting mid-file does (units, Z and the tool restored), and **From the start (line 1)**.
- Otherwise **Continue queue** starts the first file with the same raise and bit check as between files. Stop at a
  tool-change prompt keeps the bit from before, so Continue asks for the new one again.

## Arranging the queue

In G-Code ▸ Job Queue, **Up**, **Down** and **Remove** arrange the files, and **Clear queue** removes them all. The
file that is running, or being started between files, stays first until it ends or you press Stop.
