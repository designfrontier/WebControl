# Z travel limits

What this must show on the real machine: with Z limits set, no move WebControl sends takes Z past them, a job pauses
before a line that would, Touch Z stops at the lower limit, and Define Z0 and Touch Z move the limits so they stay at
the same place on the mechanism. CI replays `contract/serial/transcripts/z_lower_limit_pauses_the_job.json` and runs
every path in `wc-machine/tests/zlimits.rs`.

**This checklist moves Z. Take the bit out**, keep the router switched off at its own switch and a hand near Stop.

## 1. Set the limits

- [ ] Actions ▸ Calibration and setup ▸ Set Z-axis limits opens on 51.27 too. Both limits read "Not set".
- [ ] Jog Z up to about 5 mm below the top of its travel and press **Set upper limit here**; jog down to about 5 mm
      above the lowest safe point and press **Set lower limit here**. Settings ▸ Maslow shows the same numbers.

## 2. Moves are refused

- [ ] Jog Z up past the upper limit: nothing moves, an alert names the Z and the limit, and the log shows `G91`
      then `G90` with no `G00 Z` between them. The next jog within the limits moves.
- [ ] Type `G0 Z<below the lower limit>` in Send Custom GCode: refused with an alert. Switch to inches and repeat
      in inches: refused at the same place.
- [ ] Set the safe height above the upper limit and press Home: refused, and the sled does not travel. Put the
      safe height back.

## 3. The zero moves them

- [ ] Jog Z down 10 mm and press **Define Z0**: both limits in Settings change by 10 mm, and jogging still stops at
      the same physical heights as in section 2.
- [ ] Set Max Touch Z-axis Plunge larger than the distance to the lower limit and press Touch Z without touching
      the plate: the probe stops at the lower limit (`G90 G38.2 Z<lower limit>` in the log) and misses.
- [ ] Touch the plate during a Touch Z: the limits shift by where it touched, within about 0.25 mm.

## 4. A job

- [ ] Load a file with a Z below the lower limit and press Play (air cut): the job pauses before that line with
      "Line N would move Z to ... below the Z lower limit of ...". Resume stops there again; Stop ends it.
- [ ] Clear both limits in Settings ▸ Maslow: the same file runs past that line.
