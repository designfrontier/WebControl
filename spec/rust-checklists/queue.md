# Job queue: files back to back

What this must show on the real machine: queued files run one after another; between them Z rises to the safe
height, a file with the same tool starts at once, and a file with another tool (or none named, or an unknown tool in
the spindle) waits at the tool-change prompt; Stop, an alarm, a lost link and a controller reset stop the queue and
keep the rest queued. CI replays `contract/serial/transcripts/queue_same_tool_starts_at_once_new_tool_waits.json`,
runs every stop condition in `wc-machine/tests/queue.rs`, and runs three files on the simulator
(`wc-server/tests/queue.rs`, `frontend/e2e/queue-simulator.spec.ts`).

**This checklist moves the sled with jobs. It is an air cut.** Follow the safety lines of [`jobs.md`](jobs.md): the
router switched off at its own switch, the bit removed, Z zeroed 30 mm above the work, a hand near Stop. Run it only
after `jobs.md` sections 1 to 3 pass.

## 1. Set up

- [ ] Make three files on the Mac and upload them (G-Code ▸ Upload, top level):

      printf 'T1 M6\nG21\nG90\nG0 Z5\nG0 X0 Y0\nG1 Z-2 F100\nG1 X30 F600\nG0 Z5\n' > ~/queue-a.nc
      printf 'T1 M6\nG21\nG90\nG0 X0 Y20\nG1 Z-2 F100\nG1 X30 F600\nG0 Z5\n' > ~/queue-b.nc
      printf 'T2 M6\nG21\nG90\nG0 X0 Y40\nG1 Z-2 F100\nG1 X30 F600\nG0 Z5\nM30\n' > ~/queue-c.nc

- [ ] G-Code ▸ Open: add `queue-a.nc`, `queue-b.nc` and `queue-c.nc` with **Add to queue**. G-Code ▸ Job Queue lists
      them in that order; `queue-c.nc` reads "Tool 2, will pause for a tool change".
- [ ] Open WebControl in a second browser (a phone): it shows the same queue. Reload the page: the queue is still there.

## 2. Back to back

- [ ] Press **Start queue**. The log shows `QUEUE LOAD file=queue-a.nc`, then `G00 Z<safe>`, `M5` and `T1 M6`, and
      the tool-change prompt asks for tool 1 (the tool in the spindle is unknown after connecting).
- [ ] Press **Resume**: Z rises, and `queue-a.nc` runs.
- [ ] When it ends, `queue-b.nc` starts with no prompt: the log shows `QUEUE DONE file=queue-a.nc`,
      `QUEUE LOAD file=queue-b.nc`, `G00 Z<safe>` and `QUEUE START file=queue-b.nc`, and the sled travels at the safe
      height before `queue-b.nc`'s first move.
- [ ] When `queue-b.nc` ends, Z rises, `M5` and `T2 M6` go out and the prompt asks for tool 2. The queue line says it
      waits for the bit. Press **Resume**: `queue-c.nc` runs, and the queue ends empty (`QUEUE FINISHED`).

## 3. Stops

Re-add the three files for each check.

- [ ] Press **Stop** in the middle of `queue-a.nc`: the queue line says "Queue stopped: Stop was pressed.", nothing
      else starts, and all three files stay queued. **Continue queue** starts `queue-a.nc` from its first line.
- [ ] At the tool-2 prompt press **Stop**: the queue stops with `queue-c.nc` still queued.
- [ ] Unplug the controller's USB during `queue-a.nc`: the job pauses with Connection Lost, the queue stops ("The
      controller's connection was lost."), and when `queue-a.nc` is resumed to its end nothing else starts.
- [ ] Press the controller's reset button (or power-cycle it) during `queue-a.nc`: the queue stops ("The controller
      restarted.").
- [ ] Rename `queue-b.nc` on the Pi (`mv ~/.WebControl/gcode/queue-b.nc /tmp/`) while `queue-a.nc` runs: when it
      ends an alert says it could not load `queue-b.nc`, and the queue stops with it still queued. Move it back.

## 4. Arranging and restarts

- [ ] While `queue-a.nc` runs, its Up, Down and Remove are greyed out; moving and removing the others works.
- [ ] `sudo systemctl restart webcontrol-rs` with files queued: after the restart they are listed, the queue is idle,
      and nothing moves until you press Play or Start queue.
