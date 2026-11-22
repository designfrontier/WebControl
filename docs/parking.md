---
layout: default
title: Parking
nav_order: 6
---
# Parking

A park position is a spot the sled goes to when a job is done, out of the way of the work: for example a corner of
the bed, so you can reach the board. It is off until you set one.

## Setting the park position

- **Set Park Here** (on the control screen, or Actions ▸ Calibration and setup ▸ Set park here) stores where the
  sled is now. Jog the sled there first and wait for it to stop.
- **Settings ▸ Maslow ▸ Park Position X in MM** and **Park Position Y in MM** hold it as numbers. Clear both to turn
  parking off.

The park position is in millimetres from the centre of the bed, in machine coordinates: it is where the sled is on
the bed, not where it is in your gcode. Setting home (Set Home Here, Reset home to center) moves where files cut, but
never moves the park position.

## When the sled parks

- **A file finishes normally**: once the controller has finished the file's last line.
- **The job queue's last file finishes normally**. Between queued files the sled does not park; the next file
  starts.
- **Park** is pressed (control screen or Actions ▸ Calibration and setup), when no job is running or paused.

It never parks when a job ends any other way: Stop, an alarm, a lost USB connection, a controller restart, a line
WebControl refused to send, an `error:` from the controller, or a failed probe. Once one of those happens during a
run, that run does not park even if you resume it to the end. Pressing Pause, or sending the machine anything else (a
jog, a macro, typed gcode), after the last line went out but before the controller finished it also cancels the
park. A queue that Pause or any other stop halted does not park when its file ends.

## What it sends

WebControl sends the units the controller is in and `G90`, raises Z to the safe height (Settings ▸ Maslow ▸ Z-Axis
Safe Travel Height), or keeps the bit where it is if it is already higher, then rapids to the park position:

```
G21
G90
G00 Z5.0
G00 X-300.0 Y200.5
```

The sled travels in a straight line from wherever the job ended, as Home does: keep clamps, screws and anything
else taller than the safe height out of that path, or pick a park position whose path avoids them.

Z stays at the safe height. A file that switched the controller to inches (`G20`) parks in inches, converted from
the stored millimetres. The log and the controller messages show `PARK START trigger=file|queue|button x_mm=... y_mm=...
units=... x=... y=...`, and `PARK SET` when Set Park Here stores a position.

## When Park is refused

Park, and the park after a job, are refused with a message when:

- no park position is set, or it is off the bed;
- the safe height is not a number above 0;
- a job is running or paused, a tool change waits, or the queue is between files;
- an alarm holds the controller (press Stop first), or the controller did not restart after a `$38` change.

If a job finished but could not park, the log shows `PARK SKIPPED` with the reason, and an alert says so.
