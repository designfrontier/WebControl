---
layout: default
title: Tune the Planner Firmware
nav_order: 3
parent: Controller
grand_parent: Actions
---
# Tune the Planner Firmware

### What each setting does

All three are in Settings ▸ Maslow Settings, and appear only when the controller runs the planner firmware 51.30.

* **Planner Acceleration (`$47`)**, mm/s², default 50: how fast the sled speeds up and slows down. Higher finishes
  jobs sooner, until the sled starts to swing and the chains bounce.
* **Planner Junction Deviation (`$48`)**, mm, default 0.01: how fast corners are taken. The planner may cut a corner
  by up to this much to keep its speed through it.
* **Planner Feed-Forward (`$49`)**, 0 to 1.5, default 0 (off): sends part of the planned chain speed straight to the
  motors, so the sled lags less behind where it should be. Too high, and the sled runs past where it should stop.

### What you need

* The planner firmware flashed: Actions ▸ Upgrade to planner firmware 51.30 (see
  [Upgrade Holey Firmware](Actions/Controller/upgradeHoleyFirmware.md)).
* Settings ▸ Display ▸ "Show line pacing in the run panel" on. The run panel then shows the position error (mean and
  max) and the max overshoot of each job.
* The two test files, `planner-aircut.nc` and `planner-testcut.nc`. Download them from the WebControl repository on
  GitHub, in `spec/rust-checklists/`
  ([air cut](https://github.com/designfrontier/WebControl/blob/master/spec/rust-checklists/planner-aircut.nc),
  [test cut](https://github.com/designfrontier/WebControl/blob/master/spec/rust-checklists/planner-testcut.nc)): open
  each, then save it with the Download raw file button. Upload each with G-Code ▸ Upload.

### 1. Acceleration first, with `$49` at 0

Run the air cut at `$47` = 50, 100, 150 and 200. For the air cut, set Z 10 mm above the work, keep the spindle off,
and start from Home each time.

* Watch the sled at the corners and at the ends of the rapids for swing and chain bounce.
* After each run, write down the job time and the run panel's error mean, max and max overshoot.
* Stop raising `$47` when the worst error nears about 1.5 mm or the sled visibly swings.

Each step saves less time than the last. On one machine, 50 → 100 saved 6%, 100 → 150 2.4% and 150 → 200 2.1%,
and the error at full speed did not change.

### 2. Then feed-forward, `$49`

Run the air cut at `$49` = 0, 0.5, 0.8 and 1.0. The error at full speed should fall at each step.

Then cut the test cut at the two best values, side by side: 1 mm deep in scrap or the spoilboard, with Z zero on the
surface. Use the viewer's Position button to place the second cut beside the first, and do not press Home between
them.

* Measure each slot with calipers. Overrun = the F800 slot's length − the F100 slot's length in the same group.
* Pick the highest gain with no measurable overrun and no overshoot flare at the square's corners.

The cut matters because the logs can miss a brief overrun that the cut records, though the max overshoot now shows
most of it. On one machine, 0.8 measured about 0 overrun and 1.0 about 0.6 mm; the error at full speed went from
0.84 mm at 0 to 0.43 mm at 0.8.

### 3. Junction deviation, `$48`

Leave it at 0.01 unless corners look rounded or slow. Raising it takes corners faster but rounds them more; lowering
it makes corners sharper and slower.

### Where the values live

In the controller's EEPROM. They survive reflashing, with the upgrade wizard or `flash-hex.sh`. Going back to holey
51.292 ignores them.

### Typical starting values

For a stock Maslow, `$47` = 100–200 and `$49` = 0.5–0.8. One tuned machine runs `$47` = 200, `$48` = 0.01 and
`$49` = 0.8. Tune your own machine: chains, sled weight and frame all change the result.
