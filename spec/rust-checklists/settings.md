# Settings on the real controller

What this must show on the real machine: `webcontrol-rs` saves each edited setting to `~/.WebControl/webcontrol.json`
and writes it to the controller; after every controller boot it reads `$$` and writes only what differs, plus `$13`
(which sets the sprocket radius), never a `$38` the controller already holds (`spec/firmware.md` B2); and the setup
screens' sprocket, chain, motor and Z-limit actions run. **Sections 4 to 6 turn the motors.**

Safety, for every step below (and the README's):

- **Router switched off at its own switch, and the bit removed.**
- Sections 4 to 6 turn the sprockets and can extend chains: the sled is supported or hangs freely with room to move,
  and you can reach the controller's power.
- Keep a hand near **Stop**. If Stop ever fails, cut the controller's power and report it.

You need: the release installed, the controller on USB (holey 51.27 or 51.292) and connected (`connection.md`
section 1 passed), the log viewer open, and a browser.

## 1. Edit a setting

- [ ] Note three values: Settings ▸ Maslow, Distance Between Motors (`$2`) ______; Settings ▸ Advanced, Rotation
      Radius for Triangular Kinematics (`$8`) ______ and Max Z-Axis RPM (`$18`) ______.
- [ ] Settings ▸ Maslow: change Distance Between Motors by +1 mm and Save. The form shows the new value, and the log
      shows one `Sent: $2=<new value>` and its `ok` (on 51.292, after `Change in setting, rewriting EEPROM`).
- [ ] Reload the page: the new value is still there. On the Pi,
      `jq '."Maslow Settings"[] | select(.key == "motorSpacingX") | .value' ~/.WebControl/webcontrol.json` prints it,
      and `webcontrol.json.bak` beside it holds the previous version.
- [ ] Type `abc` into Distance Between Motors and Save: the form says "Must be a number", and nothing is sent or
      saved.
- [ ] Type the value with a decimal comma (for example `2978,4`): it is taken as `2978.4`. Type `1,500`: it is
      refused, since it reads as a thousands group. Put the value back to what you noted, and Save: one `Sent: $2=`.
- [ ] Change Units on the control screen (mm ↔ inches): the log shows `Sent: G20` (or `G21`), the jog step and the
      home position convert, and the change is still there after a reload. Change it back.

## 2. Power-cycle the controller: `$$`, then only what differs

- [ ] Switch the controller's power off and on (or unplug and replug USB). Within about 10 s the log shows the boot
      banner, `Sent: B05`, the `Firmware Version` reply, `Sent: G21` (or `G20`), `Sent: G90`, `Sent: $$` and its reply,
      then **only** `Sent: $13=63.5` (your distance per rotation). No `Sent: $38=`, and no second `$$`
      (`connect_boot_handshake`).
- [ ] The `$$` reply lists `$2`, `$8` and `$18` at the values from section 1. `$18` is `12.6` on the stock Z motor,
      never `126`.
- [ ] Jog once in each direction: the sled moves (no "Sled Not Keeping Up" alarm).
- [ ] Settings ▸ Advanced: change Top/Bottom Chain Feed and Save. The log shows `Sent: $38=...`; an alert says the
      controller is restarting; the boot banner follows; and the next handshake writes only `$13`
      (`connect_chain_over_sprocket_differs_restarts_the_controller`). Jog once. Then change it back the same way.
- [ ] No other alarm or alert appeared, and the firmware chip shows the same version as before.

## 3. Sync settings

- [ ] Actions ▸ Sync settings: the log shows `Sent: $$` and the full reply, and nothing is written after it when
      nothing differs.
- [ ] Save the reply (copy it from the Log pane) as `~/webcontrol-backup-<version>/dollar-dollar.txt`. The firmware
      upgrade checklist compares against it.

## 4. Quick configure and the setup screens

- [ ] Actions ▸ Quick configure shows your kit (Ring or Custom, the chain side, spacing and offset). Save it
      unchanged: the log shows no `Sent: $` line for values that did not change.
- [ ] Actions ▸ Set sprockets and chains and Actions ▸ Set Z-axis open and show the chain extend length.
- [ ] Actions ▸ PID tuning (advanced) shows the gains Kp, Ki and Kd for velocity and position. They match `$21` to
      `$36` in the `$$` reply from section 3.
- [ ] On 51.27: Actions ▸ Set Z-axis limits is greyed out with "Needs holey firmware 51.292 or later; the controller
      reports holey 51.27."
- [ ] On 51.292: Actions ▸ Set Z-axis limits opens. Report Z limits sends `B20` and the controller answers with both
      limits. Raise Z 5 mm and Save Z max: `Sent: $63=<Z>`. Lower Z 10 mm and Save Z min: `Sent: $64=<Z>`. Clear Z
      limits: `Sent: $63=9999` and `Sent: $64=-9999` (`z_limits_on_51_292`). Leave them cleared unless you use them.

## 5. Motors and sprockets (the motors turn)

- [ ] Actions ▸ Test motors and encoders, and confirm: the log shows `Sent: B04`, each motor turns both ways, and the
      controller's report appears in Controller Messages.
- [ ] Set sprockets and chains: turn the left sprocket +5° and −5°. The log shows `G91`, `B09 L<distance>`, `G90`,
      and the sprocket moves a little.
- [ ] Automatic sprocket alignment: the log shows `B10 L`, the controller's `[Measure: ...]`, `B10 R`, its reply,
      then `G91`, `B09 L...`, `B09 R...`, `G90`, and both sprockets end with a tooth at 12 o'clock.
- [ ] Actions ▸ Fake servo (advanced): turn it on and off. The log shows `B99 ON` and `B99 OFF` and the controller's
      reply to each, and the controller chip says "fake servo on" while it is on.

## 6. Chains (only if you are re-hanging the chains anyway)

- [ ] Extend the left chain: after the 5 s countdown the log shows `B02 L1 R0` then `B10 L`. When the measurement
      comes back WebControl sends `!` and the chain stops. Press Stop yourself if it does not stop at the extend
      length.
- [ ] Actions ▸ Reset chains, with the marked link on each top tooth: "I have reset both chains with the marked link
      on the vertical tooth" sends `B08`. Only press it with the chains really at their extend length.

## 7. Put things back

- [ ] Every value you changed is back to what you noted in section 1 (Settings ▸ Maslow and Advanced), and
      Actions ▸ Sync settings shows them in `$$`.

Report as the README says.
