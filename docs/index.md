---
layout: default
title: Home
nav_order: 1
description: Help pages for WebControl
wcversion: 0.915.001
tags: web-control
---
# WebControl

Control your [Maslow CNC](https://www.maslowcnc.com/) from any web browser on your network.

* Runs on a Raspberry Pi beside the machine and connects to the Maslow's Arduino Mega over USB.
* Includes the setup and calibration steps (updated from [Maslow Community Garden](http://maslowcommunitygarden.org/)).
* Runs gcode (`.nc` files) for cutting.

<img src="assets/Screenshot.png" alt="Screenshot" width="100%">

## Installation

This version of WebControl runs on a Raspberry Pi 3B or newer with Raspberry Pi OS Lite (64-bit). Install it from a
release with the steps in
[Install on a Pi](https://github.com/designfrontier/WebControl/blob/master/spec/INSTALL-PI.md).

## Usage

Open your web browser to `http://<your Pi's name>.local:5000/`, or use your Pi's IP address.

### Remote Access

WebControl has no login, so anyone who can reach it can move the machine. Keep it on your own network; before you put
it behind a proxy for access from elsewhere, be sure you understand the security implications.

## With Thanks

This WebControl grew out of [WebControl](https://github.com/WebControlCNC/WebControl), which was adapted from the
Kivy-based [Ground Control](https://github.com/maslowcnc/groundcontrol).

* **Madgrizzle** - *Initial work* - [madgrizzle](https://github.com/madgrizzle)
* **John Boiles** - *Docker Expert* - [johnboiles](https://github.com/johnboiles)
* **Tinker** - *UI Improvements/Bug Fixer/Etc.* - [gb0101010101](https://github.com/gb0101010101)

See also the list of [contributors](https://github.com/WebControlCNC/WebControl/graphs/contributors) who participated
in WebControl.

## License

See [LICENSE](https://github.com/designfrontier/WebControl/blob/master/LICENSE)
