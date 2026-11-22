---
layout: default
title: Job Queue
nav_order: 5
---
# Job Queue

The job queue runs gcode files one after another: when one file ends, the next one starts on its own.

## Adding files

- **G-Code ▸ Open**: choose a file and press **Add to queue**. The file is added to the end of the queue without
  being opened, so you can add files while a job runs.
- **G-Code ▸ Job Queue ▸ Add loaded file** adds the file that is loaded now.

The queue is kept on the Raspberry Pi, so every browser sees the same queue, and it is still there after you reload
the page or restart WebControl. After a restart it never starts by itself: press Play or **Start queue**.

## Running the queue

The first file in the queue is the one that runs. Start the queue in either of two ways:

- Load the first file and press **Play**, as for any job.
- Press **Start queue** (in G-Code ▸ Job Queue). This loads the first file, raises Z and checks the tool as between
  files (below), then runs the file from its first line.

When a file ends, and the controller has finished its last line, WebControl:

1. Takes the finished file off the queue and loads the next one.
2. Raises Z to the safe height (Settings ▸ Maslow ▸ Z-Axis Safe Travel Height), or keeps the bit higher if it is
   already higher.
3. Compares the first tool the next file names (its first `T` word) with the tool in the spindle:
   - **The same tool**: the next file starts at once.
   - **Another tool, or the tool in the spindle is unknown** (for example after the controller restarted): the spindle
     is turned off (`M5`) and the usual tool-change prompt asks you to insert the tool. Change the bit, set Z, and
     press **Resume**. Z rises again and the file starts.
   - **The file names no tool**: the same prompt asks you to check that the bit is the one the file needs.

The queue line beside the run controls shows what runs, what comes next and what happens between the files, for
example "Next: shelf.nc — same tool, starts immediately" or "Next: sign.nc — Tool 2, will pause for a tool change".

**Pause** and **Resume** work as for any job, and the queue goes on when the file ends.

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

Fix the cause, then press **Continue queue**. Continue starts the first file in the queue from its first line, with
the same raise and tool check as between files. If that file was stopped part way, remove it first if you do not want
it cut again.

## Arranging the queue

In G-Code ▸ Job Queue, **Up**, **Down** and **Remove** arrange the files, and **Clear queue** removes them all. The
file that is running, or being started between files, stays first until it ends or you press Stop.
