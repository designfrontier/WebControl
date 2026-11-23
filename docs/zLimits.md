---
layout: default
title: Z-Axis Travel Limits
nav_order: 8
---
# Z-Axis Travel Limits

The Z travel limits are the highest and lowest points the router may be sent to: where the Z mechanism runs out of
thread, or the router would hit the sled. The controller keeps them, so they need holey firmware 51.292 or later.

## Setting the limits

Open Actions ▸ Calibration and setup ▸ Set Z-axis limits (the Z-Axis Limits screen).

1. Press **Clear Z limits**.
2. Raise Z with the jog buttons until the router is as high as it should ever go, then press **Save Z max**.
3. Lower Z until it is as low as it should ever go, then press **Save Z min**. Take the bit out first, or make sure
   it is clear of the work.

**Report Z limits** shows them in the controller log, in the machine's units.

## They stay with the mechanism

The controller stores each limit in mm, as a Z coordinate. When Z is zeroed (**Define Z0**, **Touch Z** or a
`G10 Z...` line), the controller moves both limits by exactly the same amount, so they stay at the same place on the
mechanism, and it keeps them when it is switched off. Upgrading or downgrading the firmware resets them to 100 mm
above and below Z, so set them again after an upgrade.

## What happens at a limit

The controller holds Z at a limit, whatever is sent. WebControl also checks every Z move it sends, from the limits
the controller last reported, so that nothing is cut at the wrong depth without notice:

- **A command** past a limit (a Z jog, typed gcode, a macro, the raise before Home, Return to Center, a park or a
  restart) is not sent. An alert says where it would have moved Z and which limit it would pass, and anything queued
  after it (such as the move after a raise) is dropped too.
- **A job** pauses before the line that would pass a limit, with an alert like "Line 120 would move Z to -12.00 mm,
  below the Z lower limit of -10.00 mm." Resume stops there again: stop the job and fix the file, or set the limits
  again if they are wrong.
- **Touch Z** probes no lower than the lower limit. If the plate is not touched by then, the probe misses.

After each change of zero WebControl asks the controller for the limits again. Until it answers, and for a line
whose Z WebControl cannot follow (such as `G28`), only the controller's hold applies.

The safe height must be below the upper limit, or Home, Return to Center and Resume are refused.
