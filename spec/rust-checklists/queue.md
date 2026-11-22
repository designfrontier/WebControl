# Job queue: files back to back

What this must show on the real machine: queued files run one after another; between them Z rises to the safe
height; a file starts at once unless the files show a different bit, when it waits at the tool-change prompt; a
file's bit-change override skips every bit change in a queue run; Stop, an alarm, a lost link and a controller reset
stop the queue and keep the rest queued; Continue after a Stop part way asks where to start. CI replays
`contract/serial/transcripts/queue_same_tool_starts_at_once_new_tool_waits.json` and
`queue_override_skips_bit_changes.json`, runs every stop condition in `wc-machine/tests/queue.rs`, and runs the queue
on the simulator (`wc-server/tests/queue.rs`, `frontend/e2e/queue-simulator.spec.ts`).

**This checklist moves the sled with jobs. It is an air cut.** Follow the safety lines of [`jobs.md`](jobs.md): the
router switched off at its own switch, the bit removed, Z zeroed 30 mm above the work, a hand near Stop. Run it only
after `jobs.md` sections 1 to 3 pass.

## 1. Set up

- [ ] Make four files on the Mac and upload them (G-Code ▸ Upload, top level):

      printf 'T1 M6\nG21\nG90\nG0 Z5\nG0 X0 Y0\nG1 Z-2 F100\nG1 X30 F600\nG0 Z5\n' > ~/queue-a.nc
      printf 'T1 M6\nG21\nG90\nG0 X0 Y20\nG1 Z-2 F100\nG1 X30 F600\nG0 Z5\n' > ~/queue-b.nc
      printf 'T2 M6\nG21\nG90\nG0 X0 Y40\nG1 Z-2 F100\nG1 X30 F600\nG0 Z5\n' > ~/queue-c.nc
      printf 'T3 M6\nG21\nG90\nG0 X0 Y60\nG1 Z-2 F100\nT4 M6 G1 X30 F600\nG0 Z5\nM30\n' > ~/queue-d.nc

- [ ] G-Code ▸ Open: add `queue-a.nc`, `queue-b.nc` and `queue-c.nc` with **Add to queue**, then `queue-d.nc` with
      **Don't stop for bit changes in this file** ticked. G-Code ▸ Job Queue lists them in that order; `queue-c.nc`
      reads "Tool 2 — will pause for a bit change" and `queue-d.nc` shows "bit changes skipped" and "Tool 3 — bit
      change skipped (override)".
- [ ] Open WebControl in a second browser (a phone): it shows the same queue, and the queue line sits above Play
      with nothing covering it. Reload the page: the queue is still there.

## 2. Back to back

- [ ] Press **Start queue** right after connecting. The bit is unknown, so the log shows `QUEUE LOAD file=queue-a.nc`,
      `G00 Z<safe>` and `QUEUE START`; the file's own `T1 M6` then asks for tool 1 as in any job. Press **Resume**.
- [ ] When `queue-a.nc` ends, `queue-b.nc` starts with no prompt: `QUEUE DONE file=queue-a.nc`,
      `QUEUE LOAD file=queue-b.nc`, `G00 Z<safe>`, `QUEUE START file=queue-b.nc`, and the sled travels at the safe
      height before `queue-b.nc`'s first move.
- [ ] When `queue-b.nc` ends, Z rises, `M5` and `T2 M6` go out and the prompt asks for tool 2. Press **Resume**:
      `queue-c.nc` runs.
- [ ] `queue-d.nc` starts once Z has risen with no prompt, though it names tool 3. The log shows
      `QUEUE BIT CHANGE SKIPPED between_files tool=3`, then `file_line=1 tool=3` and `file_line=6 tool=4`; `T3 M6` is
      never sent and line 6 goes out as `G1 X30 F600`. The status bar's tool stays 2. The queue ends empty
      (`QUEUE FINISHED`).
- [ ] Play `queue-d.nc` by itself (G-Code ▸ Open, Open, Play): its `T3 M6` asks for tool 3 as usual. Stop.

## 3. Stops

Re-add the files for each check.

- [ ] Press **Stop** in the middle of `queue-a.nc`: the queue line says "Queue stopped: Stop was pressed." and offers
      **From line N (where it stopped)** and **From the start (line 1)**; nothing else starts and all files stay
      queued. **From line N** returns to that line at the safe height and goes on with the rest of the queue.
- [ ] At the tool-2 prompt press **Stop**: the queue stops with `queue-c.nc` still queued and offers only
      **Continue queue**, which asks for tool 2 again.
- [ ] Unplug the controller's USB during `queue-a.nc`: the job pauses with Connection Lost, the queue stops ("The
      controller's connection was lost."), and when `queue-a.nc` is resumed to its end nothing else starts.
- [ ] Press the controller's reset button (or power-cycle it) during `queue-a.nc`: the queue stops ("The controller
      restarted.").
- [ ] Rename `queue-b.nc` on the Pi (`mv ~/.WebControl/gcode/queue-b.nc /tmp/`) while `queue-a.nc` runs: when it
      ends an alert says it could not load `queue-b.nc`, and the queue stops with it still queued. Move it back.

## 4. Arranging and restarts

- [ ] While `queue-a.nc` runs, its Up, Down, Remove and override box are greyed out; moving, removing and ticking
      the others works.
- [ ] `sudo systemctl restart webcontrol-rs` with files queued: after the restart they are listed with their
      overrides, the queue is idle, and nothing moves until you press Play or Start queue.
