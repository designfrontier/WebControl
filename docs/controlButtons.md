---
layout: default
title: Control Buttons and Macros
nav_order: 7
---
# Control Buttons and Macros

## The four control buttons

Beside the jog arrows on the control screen are four buttons you choose: three down the right-hand side of the jog
pad and one beside the jog distance. Out of the box they are:

| Slot | Button | What it does |
| --- | --- | --- |
| Right column, top | **Park** | Raises Z and moves the sled to the park position ([Parking](parking.md)) |
| Right column, middle | **Z-Axis** | Opens the Z-Axis controls |
| Right column, bottom | **Set Home Here** | Makes the sled's position the home position |
| Beside the distance | **Set Park Here** | Makes the sled's position the park position |

Change them in **Settings ▸ Display ▸ Control buttons**. Each slot can hold Park, Z-Axis, Set Home Here, Set Park Here,
one of the four macros (listed by number and title), Go to Z0 or Go to Safe Height. A button can be in one slot only: choosing one that another
slot holds swaps the two. The choice is stored on the Raspberry Pi and applies at once on every device showing
WebControl, also during a job. The buttons keep the same size and place on a phone, an iPad and a desktop.

A button is disabled when it could not run now: Park, Set Home Here and Set Park Here while a job runs or is paused or
a tool change waits; Park also while no park position is set; a macro, Go to Z0 and Go to Safe Height while a job runs,
and a macro also when it has no gcode.
Hover over a disabled button to see why. While Park is in a slot and no park position is set, a note beside the
Controls heading says so, also on a touch screen.

## Go to Z0 and Go to Safe Height

The Z-Axis controls (and the control button slots) have two buttons that move only Z:

- **Go to Z0** lowers the bit to Z zero, where **Set Z Zero Here** or **Touch Plate** last put it. It asks first, as
  the bit goes down toward the work: make sure it is clear of clamps. It is refused until Z zero has been set since
  the controller connected or restarted. It is useful at a tool change for aligning the new bit: go to Z0 over the
  work, loosen the collet, let the new bit down onto the surface, and tighten it.
- **Go to Safe Height** raises the bit to the safe height (Settings ▸ Maslow ▸ Z-Axis Safe Travel Height), as Home and
  Park do. A bit already above the safe height stays where it is.

Both send the units line and `G90` first. They run when Raise and Lower do: not while a job runs, but while it is
paused or a tool change waits. As Raise and Lower, they are refused while anything but the line the controller paused
on waits in it (job lines, a jog pressed just before, or a move on the tool change's own line), since releasing the
pause would run those first. At a tool change parked by Park for Tool Changes, Go to Z0 first takes the sled back
over the work at the safe height, as Touch Z does; press it again to lower the bit there.

## Macros

WebControl has four macros. Each is a title and a line of gcode, set in **Settings ▸ Maslow** (Macro 1 to Macro 4,
and Macro 1 Title to Macro 4 Title). A macro sends its gcode to the controller as typed gcode would.

Run a macro from **Actions ▸ Macros**, which lists all four by title, or put it in a control button slot. A macro with
no gcode is disabled. Macros run when no job is running, and also while a job is paused, as a jog does.
