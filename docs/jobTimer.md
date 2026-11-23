---
layout: default
title: Job Timer
nav_order: 8
---

# Job Timer

WebControl times every job, to help you price your work. The Raspberry Pi keeps the time, so every browser shows the
same numbers, and reloading the page loses nothing.

## The estimate

When you open a file, the run panel shows its estimated run time after the name, such as `leg.nc in jobs · est. 1h 23m`.
WebControl adds up the time each move takes at the speed the file asks for:

- G1, G2 and G3 moves at their feed (F), limited to **Max Feedrate** (Settings ▸ Advanced).
- G0 moves at Max Feedrate.
- Z moves no faster than **Max Z-Axis RPM** times **Z-Axis Pitch**.
- Arcs by their length, and G4 dwells by their time.
- 15 ms more for each move, because the controller stops at the end of every line. A file of many short moves takes
  much longer than its feeds alone say.

Tool changes, M0 pauses and the time you take at them are not in the estimate.

## While a job runs

The **Job** row in the readouts shows, after the job's state:

- `~12m left`: the estimated time left. Once about 2% of the job is done, WebControl scales the estimate by how the
  job's real time compares with it so far, so the time left gets closer to the truth as the job runs;
- the time since the job started, pauses included;
- `cutting`: the time the job was running, without pauses and tool changes.

After the job the Job row keeps the last job's times until the next one starts.

## In the log

Each job ends with a line such as:

    PACING JOB END file_line_end=8852 mode=v0.94 gap_ms=50 ended_by=job_end wall_s=5160.2 cutting_s=4870.5 estimated_s=3420 file=jobs/leg.nc

`wall_s` is the total time, `cutting_s` the time without pauses and `estimated_s` the estimate, all in seconds. Compare
them across your jobs to see how long your machine really takes against the estimate.
