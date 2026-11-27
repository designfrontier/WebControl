---
layout: default
title: Upgrade Holey Firmware
nav_order: 2
parent: Controller
grand_parent: Actions
---
# Upgrade Holey Firmware
  
Release: >0.906
{: .label .label-blue }  

  
### Description
This function upgrades the controller firmware with a firmware image built into WebControl. There are two:

* **Holey 51.292**, the default. It enables 'Holey Calibration' and Z limits.
* **Planner firmware 51.30 (experimental)**. It is 51.292 with an acceleration planner: it plans each move's
  speed ahead, so the sled takes corners without stopping and speeds up and slows down smoothly. It adds the Planner
  Acceleration, Junction Deviation and Feed-Forward settings (see
  [Tune the Planner Firmware](Actions/Controller/tunePlannerFirmware.md)) and talks to the computer at 115200 baud.
  It is experimental: it has run on one machine so far.

**51.292 is the way back.** From 51.30, choose "Upgrade to holey 51.292" in the same screen, or roll back to the
firmware saved before the upgrade.

### Process

Choose "Upgrade to holey 51.292" or "Upgrade to planner firmware 51.30 (experimental)". WebControl then:

* Checks the board (PCB v1.0, v1.1 or v1.2 with L298 drivers only), the firmware version and both chain lengths.
  51.292 is offered from 51.27, 51.28 and 51.30; 51.30 from 51.27, 51.28 and 51.29x. Neither is offered over itself.
* Reads the controller's flash back and saves it, with the settings and chain lengths, as a backup for rollback.
* Flashes the image with avrdude, waits for the controller to boot and checks it reports the expected version (and,
  for 51.30, the planner capability). Otherwise it restores the saved firmware.
* Pushes every setting back, as the flash resets them, then asks for a motor self-test with the chains slack.

### Troubleshooting

|Result   	|Possible Cause/Solution   	|
|---	|---	|
|The upgrade is blocked   	|The message says why: the board, the version, or no image in this build of WebControl.   	|
|After 51.30, 3 s of garbage in the log   	|Normal: WebControl tries 57600 baud first, then reconnects at 115200.   	|
