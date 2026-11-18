# Calibration

What this must show on the real machine: holey calibration from your saved measurements gives the numbers
`wc-calibration` gives for the same settings, and Accept writes exactly the `$` lines it prints; the holey test
pattern air-cuts; and the computed sled position follows the sled. CI checks the maths against the golden fixtures
recorded from the original WebControl (`fixtures/calibration/`, to five decimals) and the
`holey_calibration_accept_writes_settings` transcript.

Safety, for every step below (and the README's):

- **Router switched off at its own switch, and the bit removed.**
- Section 4 cuts the test pattern in the air and moves the sled across the whole sheet: the sled hangs freely with
  room to move, and you can reach the controller's power.
- Keep a hand near **Stop**. If Stop ever fails, cut the controller's power and report it.

You need: the release installed, the controller on USB (holey 51.27 or 51.292) and connected, the log viewer open, a
browser, the Mac with this repository at the release's tag, and the twelve measurements of your last holey
calibration (from the calibration form, or the log). Triangular calibration needs stock firmware; on holey it is
checked from the Mac only (section 5).

## 1. The expected numbers, from the Mac

- [ ] Copy the settings from the Pi: `mkdir -p ~/cal-home && scp -r pi@maslow.local:.WebControl ~/cal-home/`.
- [ ] In `rust/`: `cargo run -p wc-calibration --example calibrate -- ~/cal-home holey M1 M2 … M12` with your twelve
      measurements. It prints the four values (Distance Between Motors, Motor Offset Height, the left and right chain
      tolerances), the calibration error, and the `$` lines Accept would send. Note them: ______.
- [ ] It never writes the copy: `~/cal-home/.WebControl/webcontrol.json` is unchanged.

## 2. Holey calibration: Calculate

- [ ] Actions ▸ Holey calibration opens. The "Current" column shows the values in Settings ▸ Maslow and Advanced
      (Distance Between Motors, Motor Offset Height in MM, Chain Tolerance, Left Chain and Right Chain). Note them, so
      you can put them back: ______.
- [ ] Enter the same twelve measurements and press **Calculate**. The four values are exactly the ones from section 1,
      and the calibration error matches to 6 places.
- [ ] Type `abc` into M5 and Calculate: the form refuses it, and nothing is sent. Put the number back and Calculate
      again.
- [ ] Reload the page: **Accept results** is disabled until you Calculate again.

## 3. Holey calibration: Accept

- [ ] Press **Accept results**. The page says "Accepted", and the "Current" column now shows the calculated values.
- [ ] The log shows the `$` lines section 1 printed, in its order (`Sent: $2=…`, `$3=…`, `$40=…`, `$41=…`; a value
      equal to the stored one within one part in a million has no line), each with its `ok`. On 51.292 each also has
      `Change in setting, rewriting EEPROM`.
- [ ] Actions ▸ Sync settings: the `$$` reply lists `$2`, `$3`, `$40` and `$41` at the accepted values.
- [ ] Power-cycle the controller: the handshake writes only `$13` (`connection.md` section 1), since the controller now
      holds the accepted values.

## 4. Air-cut the holey test pattern (the sled moves)

- [ ] Raise Z about 20 mm above the sheet and press **Set Z Zero Here**, so the pattern's `Z-5` plunges stay 15 mm in
      the air.
- [ ] Holey calibration ▸ **Cut test pattern**. The log shows, in order: `G21`, `G90`, `G40`, `G17`, `G0 Z5`, `M3`,
      then for each of the six holes `G0 X… Y…`, `G1 Z-5 F500`, `G0 Z5` (top middle first, then counter-clockwise),
      then `G0 X0 Y0`, `M5` (and `G20` if the machine is in inches). The controller answers `G17` with
      `Command G17 unsupported and ignored.` Z rises before every move and descends slowly at each hole; the sled ends
      in the centre.
- [ ] Cut it again and press **Stop** while the sled travels: it stops at once, and nothing more of the pattern is
      sent after the resync.
- [ ] Settings ▸ WebControl ▸ Computed Sled Position on. Jog a little: the viewer's grey computed-sled marker sits on
      the sled marker, or within a millimetre or two. Turn the setting back off.

## 5. Triangular calibration

- [ ] On holey firmware, Actions ▸ Triangular calibration is greyed out with "Needs stock firmware; the controller
      reports holey 51.27." (or `51.292`).
- [ ] From the Mac: `cargo run -p wc-calibration --example calibrate -- ~/cal-home triangular cut12 cut34 cut5
      bitDiameter` prints motor height, rotation radius and chain sag, and the lines Accept would send. They end with
      `G21 `, `G90 `, `G40 `, `G0 Z<your safe height> `, `G0 X0 Y0 `: the bit rises before the sled moves to the
      centre.

## 6. Put things back

- [ ] Keep the accepted values, or re-enter the "Current" values you noted in section 2 in Settings ▸ Maslow and
      Advanced (each save sends its `$` line).
- [ ] `rm -r ~/cal-home`.

Report as the README says.
