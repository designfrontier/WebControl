---
layout: default
title: GPIO Buttons and LEDs
nav_order: 8
---
# GPIO Buttons and LEDs

On a Raspberry Pi, **Settings ▸ GPIO** gives each header pin (GPIO2 to GPIO27) a job. Changes take effect after
WebControl restarts.

- **Buttons**: Stop, Pause, Play, Set Home, Go Home, Return to Center and Shutdown. Wire a push button from the pin to
  ground; WebControl turns on the pin's pull-up. A press does what the same button in the UI does, and is refused
  when the UI's would be. Play resumes a paused job or a waiting tool change.
- **PlayLED, PauseLED, StopLED**: lit while a job plays, while it is paused, and after Stop. Lighting one turns the
  other two off. The pin goes high to light the LED.
- **StatusLED** and **StatusLED Active Low**: one LED that shows the machine's state by how it blinks (below).
- Spindle On and Spindle Off are listed but do nothing.

## The status LED

| Blink | State |
| --- | --- |
| Very fast flicker (10 times a second) | An alarm, such as "Sled Not Keeping Up" or a lost position; press Stop to clear it |
| Two short flashes, then a pause | Not connected to the controller |
| Fast (a quarter second on, a quarter second off) | Paused: Pause, an M0, a tool change or a manual Z prompt waits for you |
| Slow (1 second on, 1 second off) | Running a G-code file |
| Steady on | Connected and idle, ready for a job |
| Off | WebControl is not running |

When several apply, the higher one in the table shows: an alarm during a paused job flickers, and a job waiting at a
tool change when the USB comes out double-flashes.

Wire it one of two ways, each with a resistor (about 330 Ω) in series with the LED:

- **StatusLED Active Low**: LED anode to 3.3 V, cathode to the pin. The pin goes low to light it.
- **StatusLED**: from the pin to the LED anode, cathode to ground. The pin goes high to light it.

You can set more than one pin to a status LED; they all show the same state. When WebControl stops it turns the LED
off and keeps the pin holding it off. If WebControl crashes or the Pi loses power, the LED may stay as it was.
