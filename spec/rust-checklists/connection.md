# The serial link: connect, jog, Z, home, Stop

What this must show on the real machine: `webcontrol-rs` opens the controller's port, resets the board, reads its
firmware version, writes only the settings that differ, and then jogs, moves Z, homes and stops as the transcripts in
`contract/serial/transcripts/` say. **This checklist moves the sled.**

Safety, for every step below (and the README's):

- **Router switched off at its own switch, and the bit removed.** Nothing here turns the spindle on, but a macro
  could.
- The sled has room to move 300 mm in every direction from where it sits, and you can reach the controller's power.
- Keep a hand near **Stop**. If Stop does not stop the sled, cut the controller's power, report it, and do not go on.

You need: the release installed (`install-update.md` section 1), the controller on USB (holey 51.27 or 51.292), the
log viewer (Help ▸ Logs) open beside the control screen, and a browser.

## 1. Connect, and the boot handshake

- [ ] Settings ▸ Maslow: note the serial port (normally `/dev/ttyACM0`).
- [ ] `sudo systemctl restart webcontrol-rs`. Within about 5 s the status bar shows "Controller: /dev/ttyACM0" and the
      firmware chip shows "Holey 51.27" or "Holey 51.29".
- [ ] The board reset when the port opened (its LEDs blink as on a power-up). The log viewer shows the boot banner
      (`PCB v1.1 Detected` or your board's, `Grbl v1.00`, `ready`), then, as `connect_boot_handshake` says:
      `Sent: B05`, `Firmware Version 51.27` (or `51.29`), `Sent: G21` (or `G20`), `Sent: G90`, `Sent: $$` and its
      reply, then `Sent: $13=63.5` (your sprocket's distance per rotation).
- [ ] No other `Sent: $n=` line follows, and no second `$$`. If one does, note the key: it is a setting the
      controller held differently from `webcontrol.json` (see `settings.md` section 2).
- [ ] On 51.292, the `$13` write is answered with `Change in setting, rewriting EEPROM`. On 51.27 it is answered with
      `ok` alone.
- [ ] If the banner never appears, note it: the handshake still goes out 3 s after the port opens
      (`connect_without_a_reset_handshakes_after_3_s`).
- [ ] No alarm and no alert. The Controller Messages pane shows the `Firmware Version` line.
- [ ] Press the reset button on the Arduino: a second banner and a second handshake, and the UI stays connected.
- [ ] Unplug the USB cable: "Controller: not connected" and the alarm "Connection Failed or Invalid Firmware". Plug it
      back in: it reconnects within 10 s, with a full handshake.

## 2. Jog, Z and home

- [ ] Set the step to 10 mm. Each of the eight jog arrows moves the sled 10 mm that way, and the position readout
      follows. The log shows `G91`, `G00 X10.0` (or `Y`, or both, signed), `G90` for each (`jog_each_direction_mm`).
- [ ] Z panel: Raise 5 mm and Lower 5 mm move Z (`G21`, `G91`, `G00 Z5.0`, `G90`). "Set Z Zero Here" sends `G10 Z0`
      and the Z readout goes to 0 (`move_z_raise_lower_and_zero_mm`).
- [ ] On a mm machine, switch the Z panel's units to inches and raise 0.25 in: Z moves 6.35 mm (`G00 Z6.35`) and the
      main units do not change (`move_z_in_inches_on_a_mm_machine`). Switch it back.
- [ ] Home: the log shows `G90`, `G00 Z<safe>`, `G00 X.. Y..`, `G00 Z0` (`home_at_safe_height_mm`). Z rises first,
      the sled travels, then Z lowers to 0.
- [ ] Raise Z 10 mm above the safe height (Settings ▸ Maslow, Z-Axis Safe Travel Height in MM), then Home again: the
      first move is `G00 Z<that height>`, not the safe height. The bit never dips before the sled travels.
- [ ] Actions ▸ Return to center: Z rises to the safe height (or stays higher), then the sled goes to `X0.0 Y0.0`
      (`return_to_center_inches`).
- [ ] If a touch plate is set up: Touch Z sends `G90 G38.2 Z-10 ...` (`touch_z_mm`). While it probes, the Job readout
      says Probing and the jog, Z and Home buttons are greyed out. Touch the plate to the bit: the controller prints
      `z axis zeroed`, Z reads 0, and no `G10 Z0` follows. Probe again and let it run out: the alert "Touch Z Failed"
      shows and Z is left alone (`touch_z_probe_fails_leaves_z_alone`).

## 3. Stop, and the alarm hold

- [ ] Set the step to 300 mm, jog right, and press **Stop** before the sled gets there: it stops at once. The log
      shows `Sent: !` right after the press, while the jog was still running.
- [ ] Straight after, jog left 10 mm: it goes out only once the controller has sent a status report (a fraction of a
      second), then moves normally.
- [ ] Z panel: Lower 50 mm and press Stop mid-move: Z stops the same way.
- [ ] Alarm hold: with the chains slack and the 12 V motor supply **off**, press Home. Within a second or two the
      alarm "Sled Not Keeping Up" shows. However long you wait, the log has no `Sent:` after the `ALARM:` line (no
      `G00 Z0`; `alarm_mid_home_holds_the_plunge_until_stop`).
- [ ] Jog once: an alert says to press Stop, and nothing is sent. Press Stop: the alarm clears. Turn the supply back
      on and jog 10 mm: it moves.

Report as the README says.
