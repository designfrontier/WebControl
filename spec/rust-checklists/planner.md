# Planner test firmware: an air cut at three accelerations

What this must show on the real machine: a test firmware with an acceleration planner runs
[`planner-aircut.nc`](planner-aircut.nc) at `$47` = 25, 50 and 100 without the sled swinging, the chains bouncing or
the position error coming near the 2 mm alarm, and how long each run takes. **This checklist flashes the controller
and moves the sled with a job.**

> **Not here yet:** the planner's streaming protocol and its settings steps come with the protocol work. Until then
> the file streams as on 51.292, and `$47` is sent by hand.

Safety, for every step below (and the README's):

- An **air cut**: router off at its own switch, bit out, Z zeroed 30 mm above the work as in `jobs.md` section 2.
  The file never goes below Z 10 and never starts the spindle.
- The flash resets the controller's EEPROM; WebControl writes every setting back when it reconnects.
- Keep a hand near **Stop**. If Stop does not stop the sled within a second, cut the controller's power.

You need: the release installed, `jobs.md` sections 2 and 3 passed on 51.292, the test firmware's hex, the log
viewer open, an ssh session on the Pi, and Settings ▸ Display ▸ "Show line pacing in the run panel" on.

## 1. Flash the test firmware

- [ ] Note the chain lengths: the last `[PE:...]` line's two last numbers in the log viewer: L ______ R ______.
- [ ] Copy the script and the hex to the Pi: `scp rust/deploy/flash-hex.sh <planner>.hex pi@maslow.local:`.
- [ ] On the Pi, as `pi`: `bash flash-hex.sh <planner>.hex`. It stops `webcontrol-rs`, avrdude ends with
      `bytes of flash verified` and `avrdude done.`, and it starts `webcontrol-rs` again.
- [ ] WebControl reconnects; the log shows `error: EEPROM read fail`, the firmware's `B05` reply and the full
      settings push. Jog Z 1 mm: the speed is as before (`$18` is 12.6, not 126).
- [ ] The `[PE:...]` chain lengths match the ones noted (within 1 mm). If not, do not run the job: put them right as
      in `firmware-upgrade.md` section 5.

## 2. The air cut at `$47` = 25, 50 and 100

- [ ] Jog to the middle of the bed and Set Home Here. The file stays within 100 mm left and right of home, and from
      100 mm below to 155 mm above it.
- [ ] Upload and open `planner-aircut.nc`. The preview shows the square, the corners above it, the two diagonal
      rapids, the circle and the zigzag.

For each value, in order:

- [ ] G-Code ▸ Send Custom GCode `$47=<value>`, then `$$`: the reply lists `$47=<value>`.
- [ ] **Play**, and watch the sled through the corners, the circle and the zigzag, and on the long rapids.
- [ ] No sled swing at the corners or at the ends of the rapids, and no chain bounce. Note what you saw.
- [ ] At the end, note the job time (`PACING JOB END ... cutting_s=`) and the `PE JOB` line from the log viewer or
      `journalctl -u webcontrol-rs | grep "PE JOB"`. The run panel's pacing stats dialog shows the same mean and
      max.
- [ ] The worst error (`max_mm`) stays well under the 2 mm alarm: stop raising `$47` once it passes 1 mm.

| `$47` | Job time, s | `mean_mm` | `max_mm` | `over_1mm` | `samples` | Swing or bounce |
| --- | --- | --- | --- | --- | --- | --- |
| 25 | | | | | | |
| 50 | | | | | | |
| 100 | | | | | | |

Run the same file once on 51.292 as well, for the job time and errors to compare against: ______ s, mean ______,
max ______.

## 3. Roll back

- [ ] Actions ▸ Upgrade Holey Firmware (51.292). The wizard refuses a controller that reports 51.29 ("The controller
      already runs 51.29x."); then flash `holey-51.292.hex` with `flash-hex.sh` as in section 1 (on the Mac,
      `bash rust/firmware/docker-build.sh <dir>` builds it).
- [ ] The firmware chip reads "Holey 51.29", Z jogs at the old speed, and the chain lengths match the ones noted.

Report as the README says, with the table and the `PE JOB` lines.
