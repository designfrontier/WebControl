# Parking

What this must show on the real machine: with a park position set, the sled parks at the safe height when a file,
or the last file of a queue, finishes normally, and when Park is pressed; it never parks between queued files or
after Stop, an alarm, a lost link or a controller reset; the park position is machine coordinates in mm, so setting
home does not move it and an inch file parks in the same place. CI replays
`contract/serial/transcripts/park_after_a_file_finishes.json` and runs every trigger and refusal in
`wc-machine/tests/park.rs` and `tests/queue.rs`, and parks on the simulator (`frontend/e2e/park-simulator.spec.ts`).

**This checklist moves the sled. It is an air cut.** Follow the safety lines of [`jobs.md`](jobs.md): the router
switched off at its own switch, the bit removed, Z zeroed 30 mm above the work, a hand near Stop. Run it after
`jobs.md` sections 1 to 3 pass.

## 1. Set the park position

- [ ] With no park position set, the control screen's **Park** button is disabled and says why (hover or screen
      reader), and Actions ▸ Calibration and setup shows "No park position yet".
- [ ] Jog the sled about 300 mm left and 200 mm up from the centre and press **Set Park Here**. The log shows
      `PARK SET x_mm=... y_mm=...` with the sled's position in mm; Settings ▸ Maslow shows the same numbers in
      Park Position X and Y. A second browser shows Park enabled at once.
- [ ] Press **Set Park Here** while a jog is still moving: it is refused ("not known yet").
- [ ] Type `5000` into Park Position X and save: Park is refused with "off the ... bed". Put the value back.

## 2. Park on demand

- [ ] Jog to the centre and press **Park**. The log shows `PARK START trigger=button`, then `G21`, `G90`,
      `G00 Z<safe>` and `G00 X... Y...`. Z rises before the sled moves, and stays up.
- [ ] Raise Z 10 mm above the safe height and press **Park**: Z does not come down before the move.
- [ ] Press **Set Home Here** somewhere else, then **Park**: the sled goes to the same place as before.
- [ ] Switch the units to inches and press **Park**: the moves go out in inches (`G20`, `G00 X-11.811 ...`) and the
      sled stops at the same place.
- [ ] Start a job and press **Park**: refused (sending gcode). Pause, press **Park**: refused (paused). Stop.

## 3. Park after a job

- [ ] Upload and play `printf 'G21\nG90\nG0 Z5\nG0 X0 Y0\nG1 Z-2 F100\nG1 X30 F600\nG0 Z5\n' > ~/park-a.nc`. When
      the last line has run the sled rises to the safe height and goes to the park position:
      `PARK START trigger=file`.
- [ ] The same file in inches (`G20`, coordinates divided by 25.4) parks at the same place, written in inches.
- [ ] Play it again and press **Stop** mid-file: the sled does not park.
- [ ] Play it again and unplug the USB mid-file; replug, resume to the end: the sled does not park.
- [ ] Queue `park-a.nc` twice (G-Code ▸ Job Queue) and start the queue: no park between the two files, one park
      after the second (`PARK START trigger=queue`).
- [ ] Set Settings ▸ Maslow ▸ Z-Axis Safe Travel Height to 0 by editing `~/.WebControl/webcontrol.json` and restart:
      the file finishes, the sled does not move, and an alert and `PARK SKIPPED` name the safe height. Put it back.
