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

Change them in **Settings ▸ Display ▸ Control buttons**. Each slot can hold Park, Z-Axis, Set Home Here, Set Park Here
or one of the four macros (listed by number and title). A button can be in one slot only: choosing one that another
slot holds swaps the two. The choice is stored on the Raspberry Pi and applies at once on every device showing
WebControl, also during a job. The buttons keep the same size and place on a phone, an iPad and a desktop.

A button is disabled when it could not run now: Park, Set Home Here and Set Park Here while a job runs or is paused or
a tool change waits; Park also while no park position is set; a macro while a job runs, or when it has no gcode.
Hover over a disabled button to see why. While Park is in a slot and no park position is set, a line under the
jog pad says so, also on a touch screen.

## Macros

WebControl has four macros. Each is a title and a line of gcode, set in **Settings ▸ Maslow** (Macro 1 to Macro 4,
and Macro 1 Title to Macro 4 Title). A macro sends its gcode to the controller as typed gcode would.

Run a macro from **Actions ▸ Macros**, which lists all four by title, or put it in a control button slot. A macro with
no gcode is disabled. Macros run when no job is running, and also while a job is paused, as a jog does.
