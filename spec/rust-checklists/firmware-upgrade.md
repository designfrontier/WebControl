# Firmware upgrade to holey 51.292, and rollback

What this must show on the real machine: the guided upgrade (Actions ▸ Upgrade holey firmware to 51.292…) takes the
controller from holey 51.27 to 51.292, puts every setting back with `$18` at the stock Z motor's 12.6 (never 51.292's
factory 126), passes the motor self-test with the chain lengths unchanged, and can roll back to the exact firmware it
read off the board. It also covers a controller that is already on 51.292. CI rehearses the wizard against `wc-sim`
with a fake avrdude (`cargo test -p wc-machine --test upgrade`, `cargo test -p wc-server --test firmware`);
`spec/firmware.md` sections 8 and C has the why.

> **v0.94 must never drive 51.292.** v0.94 never writes `$18`, so on 51.292 its Z speed is whatever the EEPROM
> holds, and after any EEPROM reset that is the factory 126, about ten times too fast. It was never run against
> 51.292. Before booting the old disk with v0.94 (the SSD, or the old card), roll the controller back to 51.27
> (section 7).

Safety, for every step below (and the README's):

- **Router switched off at its own switch, and the bit removed.** The flash resets the controller, and the
  self-test (section 4) turns every motor at full power.
- **Do not unplug USB, power off the controller, or stop `webcontrol-rs` while the wizard says "Do not unplug the
  controller or close WebControl."** The bootloader is never written (`-D`), so a broken flash can always be redone
  with avrdude (section 7), but only over USB.
- The flash resets the controller's settings (`error: EEPROM read fail`). WebControl backs them up first and writes
  them all back; the wizard's "Steps so far" and the log viewer show each step.
- Keep a hand near **Stop**. If anything moves that should not, press Stop, then cut the controller's power.

You need: the release installed (`install-update.md` section 1; a release build embeds the pinned 51.292 image), the
controller on USB and connected, the log viewer open, a browser, and about an hour. Tick each box; if one fails, note
what you saw and the wizard's "Steps so far", and go to section 8.

## 0. Which path

- [ ] Read the firmware chip in the status bar.
- **"Holey 51.27"** (or 51.28): sections 1 to 6. The first time on a new firmware pin, rehearse first: sections 1
  to 4, then the in-session rollback (section 7, part A), then sections 1 to 6 for real.
- **"Holey 51.29"**: the controller is already upgraded. Sections 1 and 2 (the check is refused, but it still reads the
  chain lengths), then 5 and 6. Section 7, part B, is the way back.

## 1. Before you start

- [ ] The README's backup is done after the last upgrade, so it holds `~/.WebControl/firmware-backups/` too. Note the
      time: `date`.
- [ ] Actions ▸ Sync settings, then save the `$$` reply from the log viewer as
      `~/webcontrol-backup-<version>/dollar-dollar-before.txt`. Note `$18`: ______ (`12.6` on the stock Z motor).
- [ ] Note the firmware chip: ______, and the boot banner in the log (`PCB v1.1 Detected` or your board's): ______.

## 2. Pre-flight: Check the controller

- [ ] Actions ▸ Upgrade holey firmware to 51.292… opens the "Upgrade holey firmware" wizard, headed "Upgrade to holey
      51.292". Its "51.292 image SHA-256" reads `d7171d97f6eef2735db488a001c556fa60e3639033e6ec7c6ef7b18ff0ee8917`, the same as
      `rust/firmware/holey-51.292.sha256`. "none in this build" means a local build without Docker: install a release.
- [ ] **Check the controller**: the log shows `Sent: B05`, `Sent: $$`, `Sent: B10 L`, `Sent: B10 R` and their replies.
- [ ] The wizard shows Board (`PCB v1.1 Detected`), Firmware before, and Chains before. Note the chains: left ______
      mm, right ______ mm.
- [ ] On 51.27: it says "Ready to upgrade PCB v1.1 Detected to 51.292" and offers **Upgrade to 51.292**.
- [ ] On 51.292: "The upgrade is blocked" with "The controller already runs 51.29x." That is expected: go to section 5.
- [ ] Any other "The upgrade is blocked" reason (a TLE9201 or TLE5206 board, an unknown banner, no banner since the
      controller connected): stop here and report it. A v1.0, v1.1 or v1.2 L298 board should pass.

## 3. Upgrade

- [ ] Tick "The router is switched off, the bit is out, and I will not unplug the controller or close WebControl
      until the upgrade reports back." and press **Upgrade to 51.292**. The wizard walks through "Backing up the
      controller" (avrdude reads the flash back: up to a minute), "Flashing the new firmware" (about 30 s), "Waiting
      for the controller to boot" and "Restoring the settings".
- [ ] It ends at "Motor self-test", saying 51.292 is running and its settings are pushed (with a short list of
      settings re-sent, if any; note them).
- [ ] The wizard's Backup line names a new directory. On the Pi, `ls ~/.WebControl/firmware-backups/<dir>` shows
      `flash-read.hex`, `rollback.hex`, `settings-dump.txt`, `chain-lengths.txt`, `webcontrol.json` and
      `preflight.txt`. Copy it to the Mac:
      `scp -r pi@maslow.local:.WebControl/firmware-backups ~/webcontrol-backup-<version>/`.
- [ ] The log shows, after the flash: the boot banner (the same board as before), `error: EEPROM read fail`,
      `Sent: B05` and `Firmware Version 51.29`, `G21` (or `G20`), `G90` and `$$`, then one `Sent: $n=...` for every
      setting **including `$18=12.6` and `$17=...`**. No `$18=126` is ever sent.
- [ ] If your `$38` differs from 51.292's factory 1, the push writes it once and the controller restarts: a second
      banner, and the push goes on without `$38` (`spec/firmware.md` B2).
- [ ] The push ends with `$63=9999`, `$64=-9999` and `Sent: $$`, whose reply lists `$18=12.60000038` (the float
      nearest 12.6), then `G21` (or `G20`) and `G90` again.
- [ ] The firmware chip reads "Holey 51.29", and Actions ▸ Set Z-axis limits is no longer greyed out.

## 4. Self-test (the motors turn)

- [ ] Let the chains go slack (support the sled), tick "The chains are slack and the sled is supported, so the motors
      can turn freely." and press **Run the motor self-test**. Each motor turns one way, then the other: L, R, then Z.
- [ ] The wizard lists L, R and Z, each with "direction 1 passed, direction 2 passed", and shows Chains after.
- [ ] It says "Every motor passed both directions." and then either "The chain lengths match pre-flight." (go on to
      section 5) or "The chain lengths differ from pre-flight" (section 5 says what to do).
- [ ] B04 turned Z, so zero Z again before any cut (Set Z Zero Here, or Touch Z).
- [ ] A motor that failed: check its wiring and **Run the self-test again**, or **Roll back to 51.27** (section 7).

## 5. Chain lengths

The controller works out where the sled is from the chain lengths it holds. If they changed, every position is off.

- [ ] Compare Chains before (section 2) with Chains after (section 4). The wizard allows 1 mm. They match: tick and
      go on.
- [ ] Already on 51.292 (no self-test this session): compare the chains section 2 read with the sled's real position
      instead. Actions ▸ Return to center: the sled stops over the sheet's centre mark (or where it always has).
- [ ] **If they differ, do not cut.** Put the chain lengths right first, one of two ways:
  - **With marked links** (the link you marked on each chain during setup): Actions ▸ Reset chains. Turn each
    sprocket until a tooth points straight up, put the marked link back on that tooth, and press "I have reset both
    chains with the marked link on the vertical tooth". This sends `B08`: both chains are now at the extend length.
  - **Without marked links:** re-hang the chains as at setup: Actions ▸ Set sprockets and chains, a tooth vertical on
    each sprocket, **Define zero**, then extend each chain (`settings.md` section 6), and hang the sled again.
- [ ] Then Actions ▸ Return to center and Home: the sled stops where it did before the upgrade. If it does not, run a
      holey calibration (`calibration.md`) before cutting.

## 6. On 51.292

- [ ] Actions ▸ Sync settings: the `$$` reply lists `$18=12.60000038`, and every key in
      `dollar-dollar-before.txt` has the same value (`$45` may differ in its last digits). Keys new in 51.292
      (`$60`, `$61`, `$63`, `$64`) are extra.
- [ ] Jog Z down 1 mm and up 1 mm: it takes as long as it did on 51.27. **A Z that moves ten times faster means `$18`
      is 126**: press Stop, and go to section 8.
- [ ] Power-cycle the controller: the handshake shows `Firmware Version 51.29`, `$$`, then only `$13=63.5`, answered
      with `Change in setting, rewriting EEPROM`. No `$18` write: the controller holds 12.6.
- [ ] Actions ▸ Set Z-axis limits works (`settings.md` section 4).
- [ ] Home, Return to center and a jog in every direction land where they did on 51.27.
- [ ] Air-cut `jobs.md` section 3 before the first real cut on 51.292.

## 7. Roll back to 51.27

Roll back before going back to v0.94, or when 51.292 misbehaves. Each flash resets the EEPROM again, and WebControl
writes every setting back.

**A. In the same session as the upgrade**, the wizard offers **Roll back to 51.27** until `webcontrol-rs` restarts.

- [ ] Press it. The wizard shows "Restoring the saved firmware", "Waiting for the controller to boot" and "Restoring
      the settings", then "Rolled back", and an alert says "Rolled back to 51.27".
- [ ] The log shows the board's banner, `Firmware Version 51.27`, `$$`, the full settings push (no `$63` or `$64`
      this time), the restart its `$38` asks for, the banner again, a push without `$38`, and `$$`.

**B. After `webcontrol-rs` has restarted**, the wizard no longer has the image, so flash the backup by hand. On the Pi:

- [ ] `ls ~/.WebControl/firmware-backups/`, and pick the directory of the upgrade: its `preflight.txt` names 51.27 as
      the firmware before.
- [ ] Flash it, with WebControl stopped so avrdude has the port. `-P` is the serial port from Settings ▸ Maslow
      (Serial Connection), usually `/dev/ttyACM0`:

  ```sh
  sudo systemctl stop webcontrol-rs
  avrdude -patmega2560 -cwiring -P/dev/ttyACM0 -b115200 -D \
    -Uflash:w:$HOME/.WebControl/firmware-backups/<dir>/rollback.hex:i
  sudo systemctl start webcontrol-rs
  ```

- [ ] avrdude reports `bytes of flash verified` and ends with `avrdude done.` WebControl connects, the log shows
      `error: EEPROM read fail`, `Firmware Version 51.27`, the full settings push, one restart for `$38`, and `$$`.

**C. If no backup is left**, rebuild the 51.27 that v0.94 bundled (madgrizzle/Firmware @ 950fb23), with a newer
compiler. It is not the owner's firmware: the owner's controller ran an older "51.27" without B99 (FAKE_SERVO) or
the TLE9201 boot line, and no build here reproduces it. The backup's `rollback.hex` is the only exact copy, so keep a
copy of it off the Pi. On the Mac, with Docker, from the repository root:

- [ ] `bash rust/firmware/docker-build.sh ~/holey-51.27 51.27` ends with
      `/out/holey-51.27.hex: <sha>`, the same SHA-256 as `rust/firmware/holey-51.27.sha256` (it fails otherwise).
- [ ] `scp ~/holey-51.27/holey-51.27.hex pi@maslow.local:` and, on the Pi, `sha256sum holey-51.27.hex` prints that
      SHA-256 again.
- [ ] Flash it as in part B, with `-Uflash:w:$HOME/holey-51.27.hex:i`, and check the same replies.

**Whichever part you used:**

- [ ] The firmware chip reads "Holey 51.27". Actions ▸ Sync settings: the reply matches the 51.27 `$$` the upgrade
      saved, `settings-dump.txt` in the backup directory (`$45` may differ in its last digits).
- [ ] Jog Z 1 mm: the speed is as before. Check the chain lengths as in section 5.
- [ ] Only now may the Pi boot v0.94 from the old disk.

## 8. If something goes wrong

- The wizard rolls back by itself when the flash fails (a lost avrdude sync is retried once first), when the board's
  banner changes after the flash, or when the controller does not answer B05 with 51.29 within 30 s. Wait for "Rolled back" or "The upgrade
  failed".
- "The upgrade failed" names the backup's `rollback.hex`: flash it as in section 7, part B.
- `$18` at 126, or Z running too fast: press Stop. Settings ▸ Advanced, Max Z-Axis RPM must read 12.6; Save it, and
  check the log shows `Sent: $18=12.6`. If it will not stay, roll back (section 7).
- Never boot v0.94 against a controller still on 51.292.

Report as the README says, with the wizard's "Steps so far" and the backup directory.
