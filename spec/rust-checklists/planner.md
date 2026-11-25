# Planner test firmware: an air cut at three accelerations

What this must show on the real machine: a test firmware with an acceleration planner runs
[`planner-aircut.nc`](planner-aircut.nc) at `$47` = 25, 50 and 100 without the sled swinging, the chains bouncing or
the position error coming near the 2 mm alarm, and how long each run takes. **This checklist flashes the controller
and moves the sled with a job.**

Safety, for every step below (and the README's):

- An **air cut**: router off at its own switch, bit out, Z zeroed 30 mm above the work as in `jobs.md` section 2.
  The file never goes below Z 10 and never starts the spindle.
- The planner build keeps 51.292's settings (same EEPROM layout), and `$47`/`$48` start at 50 and 0.01.
- Keep a hand near **Stop**. If Stop does not stop the sled within a second, cut the controller's power.

You need: the release installed, `jobs.md` sections 2 and 3 passed on 51.292, the test firmware's hex, the log
viewer open, an ssh session on the Pi, and Settings ▸ Display ▸ "Show line pacing in the run panel" on.

## 1. Flash the test firmware

- [ ] Note the chain lengths: the last `[PE:...]` line's two last numbers in the log viewer: L ______ R ______.
- [ ] Copy the script and the hex to the Pi: `scp rust/deploy/flash-hex.sh <planner>.hex pi@maslow.local:`.
- [ ] On the Pi, as `pi`: `bash flash-hex.sh <planner>.hex`. It stops `webcontrol-rs`, avrdude ends with
      `bytes of flash verified` and `avrdude done.`, and it starts `webcontrol-rs` again.
- [ ] WebControl reconnects. Jog Z 1 mm: the speed is as before (`$18` is 12.6, not 126).
- [ ] The `[PE:...]` chain lengths match the ones noted (within 1 mm). If not, do not run the job: put them right as
      in `firmware-upgrade.md` section 5.

## 2. WebControl detects the build

- [ ] The log shows `B05`, then `Firmware Version 51.29`, `Capabilities: planner line-numbers accel-settings
      status-line` and `ok`, then `Sent: M110 N1` and `Sent: N1 G21*27`: every line after it goes as
      `N<n> ...*<sum>` and is answered `ok N<n>`.
- [ ] Settings ▸ Maslow Settings lists Planner Acceleration (50) and Planner Junction Deviation (0.01). On 51.292
      they are not there.
- [ ] Jog each way once: the sled moves as before and the log shows no `rs N` lines.

## 3. The air cut at `$47` = 25, 50 and 100

- [ ] Jog to the middle of the bed and Set Home Here. The file stays within 100 mm left and right of home, and from
      100 mm below to 155 mm above it.
- [ ] Upload and open `planner-aircut.nc`. The preview shows the square, the corners above it, the two diagonal
      rapids, the circle and the zigzag.

For each value, in order:

- [ ] Settings ▸ Maslow Settings ▸ Planner Acceleration `<value>`, Save: the log shows `Sent: N.. $47=<value>*..`
      and `ok N..`. G-Code ▸ Send Custom GCode `$$`: the reply lists `$47=<value>`.
- [ ] **Play**, and watch the sled through the corners, the circle and the zigzag, and on the long rapids.
- [ ] No sled swing at the corners or at the ends of the rapids, and no chain bounce. Note what you saw.
- [ ] The run panel's current line follows the sled, not the lines sent ahead of it.
- [ ] The job ends when the sled stops, not when the last line is sent: the log shows `Sent: N.. G4 P0*..`, and
      `PACING JOB END` only after its `ok N..`.
- [ ] Note the job time (`PACING JOB END ... cutting_s=`) and the `PE JOB` line from the log viewer or
      `journalctl -u webcontrol-rs | grep "PE JOB"`. The run panel's pacing stats dialog shows the same mean and
      max. `lost=` counts each `rs` and each line sent again; note it if not 0.
- [ ] The worst error (`max_mm`) stays well under the 2 mm alarm: stop raising `$47` once it passes 1 mm.

| `$47` | Job time, s | `mean_mm` | `max_mm` | `over_1mm` | `samples` | Swing or bounce |
| --- | --- | --- | --- | --- | --- | --- |
| 25 | | | | | | |
| 50 | | | | | | |
| 100 | | | | | | |

Run the same file once on 51.292 as well, for the job time and errors to compare against: ______ s, mean ______,
max ______.

## 4. Stop and resume mid air cut

- [ ] At `$47` = 50, Play, and press **Stop** halfway round the circle. The sled stops within a second, and the
      current line is the quarter circle it was on, not the last line sent. Note it: line ______.
- [ ] Press **Play**: the log shows `M110 N<n>` then `N<n> ...`. The sled stays at Z 10 (above the safe height),
      travels to where that quarter circle starts and goes on from there to the end. Nothing before it runs again.

## 5. Roll back

- [ ] Actions ▸ Upgrade Holey Firmware (51.292), then Check: the wizard is ready to upgrade the planner build (it
      refuses only a 51.29 without the planner capability, with "The controller already runs 51.29x."). Upgrade, and
      it backs up the planner build, flashes 51.292 and pushes the settings back. Planner Acceleration leaves
      Maslow Settings.
- [ ] If the wizard refuses or fails, flash `holey-51.292.hex` with `flash-hex.sh` as in section 1 instead (on the
      Mac, `bash rust/firmware/docker-build.sh <dir>` builds it).
- [ ] The firmware chip reads "Holey 51.29", Z jogs at the old speed, and the chain lengths match the ones noted.

Report as the README says, with the table and the `PE JOB` lines.
