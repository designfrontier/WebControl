# Lost-line recovery and line pacing

What this must show on the real machine: a line the controller never received is sent again once, and a line lost
twice pauses the job at it (`spec/firmware.md` B18); the line pacing can be chosen from the run panel before Play and
mid-job; each pacing segment ends with a `PACING` line in the logs; and the run panel's stats match those lines
(`spec/firmware.md` B3). **This checklist moves the sled with a job.**

On the owner's 51.27 with the router on, waiting only for the rest of a 50 ms gap lost about 1 line in 290, while
v0.94's pacing (the default, "v0.94 50 ms") lost none in 165,876 lines. Air cuts lost none under either. So an air
cut proves the controls and the numbers; lost lines show up mostly in real cuts.

Safety, for every step below (and the README's):

- Sections 2 to 4 are an **air cut**, set up as in `jobs.md` section 2: router off at its own switch, bit out, Z
  zeroed 30 mm above the work.
- Section 5 cuts scrap with the router on, and only after `jobs.md` section 7 has passed.
- Keep a hand near **Stop**. If Stop does not stop the sled within a second, cut the controller's power.

You need: the release installed, the controller connected, `jobs.md` sections 2 and 3 passed, the log viewer open
beside the control screen, a second device (phone or iPad) showing the control screen, and an ssh session on the Pi
running `journalctl -u webcontrol-rs -f`.

## 1. Lost-line recovery

Check this whenever a job loses a line, in any checklist. The Controller Messages pane shows each one, and on the
Pi `journalctl -u webcontrol-rs | grep "never received the lines"` finds them.

- [ ] A lost line looks like this in the log: `Sent: <line>`, then no echo and no `ok`, only status reports
      (`<Idle,MPos:...>` with the same MPos, and `[PE:...]` with a free space of 127) for about 0.6 to 0.8 s; then the
      same `Sent: <line>` again, its echo and its `ok`. Controller Messages shows "Line <n> did not reach the
      controller, so it was sent again: <line>" (`stream_lost_line_is_sent_again`).
- [ ] The job carries on by itself, the sled does not stop for longer than that second, and the cut shows no gap or
      double pass at that line.
- [ ] The run panel's "Lost" count went up by one for each such message.
- [ ] A line lost twice shows the alert "Line Not Received": "The controller did not receive line <n> (<line>), even
      when it was sent again. The job is paused at that line. Press Resume to send it again, or Stop." The job is
      paused (Resume shown). **Resume** returns to the file at that line at the safe height and sends it
      (`stream_line_lost_twice_pauses_until_resume`).
- [ ] A jog or Z move lost twice is dropped with the commands queued after it, with a "Line Not Received" alert, and
      the log shows `Sent: G21` (or `G20`) and `Sent: G90` to put the controller back in step.

## 2. Choosing the pacing (no job)

- [ ] Make a file of 3,000 short moves on the Mac and upload it:
      `awk 'BEGIN{print "G21\nG90\nG0 Z5\nG0 X0 Y0\nG1 Z-1 F100"; for(i=0;i<3000;i++){r=20+i*.01; printf "G1 X%.4f Y%.4f F1200\n", r*cos(i*.05), r*sin(i*.05)}; print "G0 Z5\nG0 X0 Y0"}' > ~/pacing.nc`
- [ ] The run panel has a "Pacing" choice beside the alarm status, reading "v0.94 50 ms" (the default), and a stats
      button reading "Pacing stats" before any job has run.
- [ ] The choices are v0.94 50 ms, Min 50 ms, Min 25 ms, Min 0 ms and Custom…. Choose Min 25 ms. The second device
      shows Min 25 ms within a couple of seconds. Settings ▸ WebControl shows Line pacing `minimum gap` and Line pacing
      gap (ms) `25`, and no `Sent: $` line appears: the pacing is not a controller setting.
- [ ] Choose Custom…: a mode choice and a gap field appear. Type `501`, then `2.5`: Apply stays disabled. Type `10`
      with mode `minimum gap` and Apply: the choice reads "min gap 10 ms".
- [ ] Reload the page: the choice still reads "min gap 10 ms". Choose v0.94 50 ms again.

## 3. A job across pacings (air cut)

- [ ] Open `~/pacing.nc` and **Play**. The log, the journal and the Controller Messages each show
      `PACING JOB START file_line=1 mode=v0.94 gap_ms=50`.
- [ ] The stats button shows the lines per minute and "Lost 0 (0.00%)", and both devices update about every 2 s.
- [ ] After about a minute, choose Min 25 ms while the job runs. It is not refused, and from the next line on the
      log shows the lines going out closer together. The logs show a `PACING SEGMENT` line with `mode=v0.94
      gap_ms=50 ... ended_by=pacing_change`, then `PACING CHANGE file_line=<n> mode=minimum_gap gap_ms=25
      from_mode=v0.94 from_gap_ms=50`.
- [ ] Every word after the upper-case label is a `key=value` pair, and unknown numbers read `none`. The segment line
      has `lines`, `lost`, `lost_twice`, `lost_pct`, `lines_per_min`, `s_per_line`, `recovery_s`,
      `median_short_gap_ms`, `duration_s`, `file_line_start`, `file_line_end` and `ended_by`.
- [ ] **Pause** for 30 s, choose Min 0 ms while paused, then **Resume**. The `PACING CHANGE` line names `gap_ms=0`,
      and the Min 25 ms segment's `duration_s` does not count the 30 s pause.
- [ ] Let the job finish. The logs show the last `PACING SEGMENT ... ended_by=job_end`, a `PACING JOB` line for the
      whole job, and `PACING JOB END file_line_end=<last line> ... ended_by=job_end`.
- [ ] The job's `lines` is the number of file lines streamed (3,007, without resends), and its `duration_s` is
      within a few seconds of the time the job took, less the pause. Note `lines_per_min` for each segment:
      v0.94 50 ms ______, Min 25 ms ______, Min 0 ms ______.
- [ ] Play again and press **Stop** after a few seconds: the segment and job lines end with `ended_by=stop`.

## 4. The stats dialog

- [ ] After the job, the stats button reads "Last job · Lost <n>". Press it: the "Line pacing" dialog shows "Pacing
      now: ...", "Last job: <n> lines/min · ..." and the table "Ended segments, newest first".
- [ ] The table has one row per segment from section 3, newest first, at most 10, with the pacing, lines per minute,
      lines, lost (with the percentage), s/line, recovery seconds, the median short gap, minutes and what ended it.
      Its numbers match the `PACING SEGMENT` lines.
- [ ] On a portrait iPad or a phone, the table scrolls inside the dialog and the dialog closes with its Close button.
- [ ] Reload the page, and open the control screen on the second device: both show the same last job.
- [ ] Job timer: before Play, the run panel's file reads `· est. <time>`. During the job the readouts' **Job** row
      shows `~<time> left`, wall time and `cutting <time>`, the same on the second device and after a reload; a Pause
      adds to wall time only. At the end `PACING JOB END` gives `wall_s`, `cutting_s`, `estimated_s` and `file`, and
      the Job row keeps the last job's times. Note estimated ______ s against cutting ______ s.
- [ ] Choose v0.94 50 ms again, unless you are running the experiment below.

## 5. Optional: the loss-rate experiment (router on)

Only after `jobs.md` section 7 has passed. On scrap, clamped flat, with the bit fitted and Z zeroed on the surface.

- [ ] Cut `~/pacing.nc` once per pacing, moving home a little each time so the cuts do not overlap. Change the
      pacing before Play, not during the job, so each job is one segment.
- [ ] Note each `PACING JOB` line's numbers: v0.94 50 ms: lost ______ of ______, ______ lines/min; Min 25 ms: lost
      ______ of ______, ______ lines/min; Min 0 ms: lost ______ of ______, ______ lines/min.
- [ ] Each lost line follows section 1, and none left a mark on the cut.
- [ ] Put the pacing back to v0.94 50 ms.

Report as the README says, with the `PACING` lines from `~/.WebControl/webcontrol-rs.log`.
