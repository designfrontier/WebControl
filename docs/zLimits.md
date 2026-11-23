---
layout: default
title: Z-Axis Travel Limits
nav_order: 8
---
# Z-Axis Travel Limits

The Z travel limits are the highest and lowest points the router may be sent to: where the Z mechanism runs out of
thread, or the router would hit the sled. With them set, WebControl refuses any move past them before it reaches the
controller. They are off until you set them, and they work on every firmware.

## Setting the limits

Open Actions ▸ Calibration and setup ▸ Set Z-axis limits (the Z-Axis Limits screen).

1. Raise Z with the jog buttons until the router is as high as it should ever go, then press
   **Set upper limit here**.
2. Lower Z until it is as low as it should ever go, then press **Set lower limit here**. Take the bit out first, or
   make sure it is clear of the work.

Each limit is stored in mm in **Settings ▸ Maslow ▸ Z-Axis Upper Limit in MM** and **Z-Axis Lower Limit in MM**. You
can type them there too. Clear one to remove that limit.

## They stay with the mechanism

The limits are Z coordinates, so they are measured from the current Z zero. When you zero Z, with **Define Z0**,
**Touch Z** or a `G10 Z...` line, WebControl moves both limits by the same amount. The numbers in Settings change, but
the limits stay at the same place on the mechanism.

After a Touch Z, WebControl takes the place the plate was touched from the last status report before the touch. The
probe moves at 60 mm/min and the controller reports about every 0.2 s, so that is good to about 0.25 mm. Leave a
little room between each limit and where the mechanism really stops.

WebControl only knows about the zero changes it sends. If Z moves while WebControl is not connected (the controller
was moved by hand, flashed or reset its settings), set the limits again.

## What is refused

Every Z move WebControl sends is checked: the Z jog buttons, typed gcode, macros, the raise before Home, Return to
Center, a park or a restart, and every line of a job, in mm or inches and absolute or relative.

- **A command** past a limit is not sent. An alert says where it would have moved Z and which limit it would pass,
  and anything queued after it (such as the move after a raise) is dropped too.
- **A job** pauses before the line that would pass a limit, with an alert like "Line 120 would move Z to -12.00 mm,
  below the Z lower limit of -10.00 mm." Resume stops there again: stop the job and fix the file, or change the
  limits if they are wrong.
- **Touch Z** probes no lower than the lower limit. If the plate is not touched by then, the probe misses.
- If WebControl cannot tell where a line would take Z (Z unknown after a stop, or a code it does not follow, such as
  `G28`), the line is refused while a limit is set. Wait a moment for a status report and try again.

The safe height must be below the upper limit, or Home, Return to Center and Resume are refused.

## Controller limits on firmware 51.292

Holey firmware 51.292 and later can also keep limits of their own (`$63` and `$64`), set on the same screen under
"Limits the controller keeps". WebControl's limits do not need them.
