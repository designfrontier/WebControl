# Planner firmware 51.30: an air cut at three accelerations

What this must show on the real machine: the planner firmware 51.30, flashed from the upgrade wizard, runs
[`planner-aircut.nc`](planner-aircut.nc) at `$47` = 25, 50 and 100 without the sled swinging, the chains bouncing or
the position error coming near the 2 mm alarm, and how long each run takes. **This checklist flashes the controller
and moves the sled with a job.**

Safety, for every step below (and the README's):

- An **air cut**: router off at its own switch, bit out, Z zeroed 30 mm above the work as in `jobs.md` section 2.
  The file never goes below Z 10 and never starts the spindle.
- The flash resets the controller's settings, and the wizard pushes them back; `$47`/`$48`/`$49` start at 50, 0.01
  and 0.
- Keep a hand near **Stop**. If Stop does not stop the sled within a second, cut the controller's power.

You need: the release installed, `jobs.md` sections 2 and 3 passed on 51.292, the log viewer open, an ssh session on
the Pi, and Settings ▸ Display ▸ "Show line pacing in the run panel" on.

## 1. Flash the planner firmware

- [ ] Note the chain lengths: the last `[PE:...]` line's two last numbers in the log viewer: L ______ R ______.
- [ ] Actions ▸ Upgrade holey firmware…: the wizard says the planner firmware is experimental and that 51.292 is the
      way back, with a Help link to the upgrade page. Press **Upgrade to planner firmware 51.30 (experimental)**: it
      checks the controller and reads "Ready to upgrade ... to 51.30", with the 51.30 image's SHA-256 matching
      `rust/firmware/holey-51.30.sha256`.
- [ ] Tick the box and **Upgrade to 51.30**: it backs up, flashes, and the log shows garbage for 3 s, then
      `Connected on port` a second time at 115200, the boot banner, `Firmware Version 51.30` and the `Capabilities:`
      line. The wizard pushes the settings back and asks for the motor self-test; run it with the chains slack.
- [ ] Restart `webcontrol-rs` (`sudo systemctl restart webcontrol-rs`): it connects at once, with no garbage, as it
      opens at the rate that last answered.
- [ ] Fallback, if the wizard refuses or fails: copy `rust/deploy/flash-hex.sh` and the hex
      (`bash rust/firmware/docker-build.sh <dir> 51.30` builds `holey-51.30.hex`) to the Pi and run
      `bash flash-hex.sh holey-51.30.hex` as `pi`. It stops `webcontrol-rs`, avrdude ends with
      `bytes of flash verified` and `avrdude done.`, and it starts `webcontrol-rs` again.
- [ ] Jog Z 1 mm: the speed is as before (`$18` is 12.6, not 126).
- [ ] The `[PE:...]` chain lengths match the ones noted (within 1 mm). If not, do not run the job: put them right as
      in `firmware-upgrade.md` section 5.

## 2. WebControl detects the build

- [ ] The log shows `B05`, then `Firmware Version 51.30` (51.29 before the `peak-error` build), `Capabilities:
      planner line-numbers accel-settings status-line feed-forward peak-error` (0f927ec stops before `peak-error`,
      c77bd97 before `feed-forward`) and `ok`, then `Sent: M110 N1` and `Sent: N1 G21*27`: every line after it goes
      as `N<n> ...*<sum>` and is answered `ok N<n>`.
- [ ] Settings ▸ Maslow Settings lists Planner Acceleration (50), Planner Junction Deviation (0.01) and Planner
      Feed-Forward (0). On 51.292 they are not there, and on c77bd97 Planner Feed-Forward is not.
- [ ] Jog each way once: the sled moves as before and the log shows no `rs N` lines.

## 3. The air cut at `$47` = 25, 50 and 100

This records the release's validation. To choose values for a machine, follow the in-app help page Tune the Planner
Firmware (`docs/Actions/Controller/tunePlannerFirmware.md`) instead.

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

## 4. Feed-forward at `$49` = 0, 0.5, 0.8 and 1.0

On the `feed-forward` build only, at `$47` = 50. `$49` adds that fraction of each chain's planned speed to its motor
command, so the chains should lag the path less: `PE JOB`'s `mean_mm` should fall as `$49` rises. Too much shows as
the sled overshooting where it stops.

For each value, in order:

- [ ] Settings ▸ Maslow Settings ▸ Planner Feed-Forward `<value>`, Save: the log shows `Sent: N.. $49=<value>*..`
      and `ok N..`. `$$` lists `$49=<value>`.
- [ ] **Play**, and watch the corners and the ends of the rapids: no overshoot past a corner or a stop, no buzz or
      chatter from the motors, no chain bounce.
- [ ] Note the job time and the `PE JOB` line as in section 3.
- [ ] On the `peak-error` build, `PE JOB` also ends with `max_ahead_mm` (the furthest a chain ran past its setpoint)
      and `overshoots` (reports more than 0.2 mm past), and the pacing stats show "overshoot max X mm". These catch
      the overshoot that comes and goes between the 200 ms reports, which before showed only in a measured cut. Cut
      `planner-testcut.nc` at the chosen value as well, and compare its overrun (each fast slot's length less the
      F100 slot's) with `max_ahead_mm`.
- [ ] Stop raising `$49` if `max_mm` grows, `max_ahead_mm` passes 0.2, the sled overshoots or the motors buzz, and
      set back the last good value.

| `$49` | Job time, s | `mean_mm` | `max_mm` | `over_1mm` | `samples` | `max_ahead_mm` | `overshoots` | Overshoot, buzz or bounce |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | | | | | | | | |
| 0.5 | | | | | | | | |
| 0.8 | | | | | | | | |
| 1.0 | | | | | | | | |

Leave `$49` at the best value for section 5.

## 5. Stop and resume mid air cut

- [ ] At `$47` = 50, Play, and press **Stop** halfway round the circle. The sled stops within a second, and the
      current line is the quarter circle it was on, not the last line sent. Note it: line ______.
- [ ] Press **Play**: a dialog reads "Stopped at line N. Resume there, or start over from line 1?" with that line.
      **Cancel** closes it and nothing moves.
- [ ] Press **Play**, then **Resume at line N**: the log shows `M110 N<n>` then `N<n> ...`. The sled stays at Z 10
      (above the safe height), travels to where that quarter circle starts and goes on from there to the end. Nothing
      before it runs again.
- [ ] Press **Stop** again partway, then **Play** and **Start over**: the file runs from line 1.

## 6. Roll back

- [ ] Actions ▸ Upgrade holey firmware…, then **Upgrade to planner firmware 51.30 (experimental)** again: it is
      blocked with "The controller already runs 51.30." **Start over**.
- [ ] **Upgrade to holey 51.292**: the wizard is ready to upgrade the planner build (it refuses only a 51.29 without
      the planner capability, with "The controller already runs 51.29x."; 51.30 is accepted by its version).
      Upgrade, and
      it backs up the planner build, flashes 51.292 and pushes the settings back. Planner Acceleration and
      Planner Feed-Forward leave Maslow Settings.
- [ ] If the wizard refuses or fails, flash `holey-51.292.hex` with `flash-hex.sh` as in section 1 instead (on the
      Mac, `bash rust/firmware/docker-build.sh <dir>` builds it).
- [ ] The firmware chip reads "Holey 51.29", Z jogs at the old speed, and the chain lengths match the ones noted.
- [ ] After a rollback from a 51.30 build, WebControl reconnected at 57600: the log shows 3 s of garbage at 115200,
      then the 51.292 banner. A restart of `webcontrol-rs` connects at once.

Report as the README says, with the table and the `PE JOB` lines.
