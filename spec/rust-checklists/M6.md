# M6 hardware checklist: the holey 51.292 upgrade, a rollback rehearsal, then the real upgrade

What M6 must show on the real machine: `webcontrol-rs` upgrades the controller from holey 51.27 to 51.292 with the
guided upgrade (Actions → Upgrade holey firmware to 51.292…), restores every setting afterwards with `$18` at the
stock Z motor's 12.6 (never 51.292's factory 126), rolls back to the exact firmware it read off the board, and
leaves the Python install untouched as the fallback. The order matters: **upgrade, roll back, and only then upgrade
for real** (sections 4 to 7). CI already rehearses all of this against `wc-sim` with a fake avrdude
(`cargo test -p wc-machine --test upgrade`, `cargo test -p wc-server --test firmware`).

Safety, for every step below:

- **Router switched off at its own switch, and the bit removed.** The flash resets the controller, and the
  self-test (section 5) turns every motor at full power.
- **Do not unplug USB, power off the controller, or stop `webcontrol-rs` while the wizard says "Do not unplug".**
  The bootloader is never written (`-D`), so a broken flash can always be redone with avrdude (section 8), but only
  over USB.
- The flash resets the controller's settings (`error: EEPROM read fail`). WebControl backs them up first and pushes
  them all back; the step-by-step log in the wizard and the log viewer (`/logs`) show each line.
- Keep a hand near **Stop**. If anything moves that should not, press Stop, then cut the controller's power.

You need: the Mac with this branch checked out and Docker running (the build makes the 51.292 image from its tag),
the Pi booted from the new microSD card (`pi@maslow.local`), the controller on USB (holey 51.27 on PCB v1.1),
the Python service installed and working, a browser, and about an hour.

Tick each box; if one fails, note what you saw and the wizard's "Steps so far", and go to section 8.

## 1. Before you start

- [ ] On Python: connect, and check that About shows "Firmware: Holey 51.27" and the log shows `PCB v1.1 Detected`.
      Close the browser tab.
- [ ] Back up the settings to a new folder (never edit the original):
      `scp -r pi@maslow.local:.WebControl ~/webcontrol-backup-m6/`.
- [ ] On Python, Actions → Sync settings, then save the `$$` reply from the log viewer as
      `~/webcontrol-backup-m6/dollar-dollar-51.27.txt`. Note `$18` (it should be `12.6`). Note the time: `date`.

## 2. Install and switch (from the Mac, repo root)

- [ ] `rust/deploy.sh install` builds the 51.292 image in Docker first (`rust/firmware/docker-build.sh`, about two
      minutes the first time) and ends with `Installed webcontrol-rs 0.1.0 (<commit>, contract 3) for pi.` If it
      says "no docker", start Docker and run it again: without the image the wizard refuses to upgrade.
- [ ] On the Pi: `avrdude -? 2>&1 | head -1` prints avrdude's usage (install-pi.sh installed it).
- [ ] On the Pi: `wc-switch rust` prints "WebControl is now the rust backend". The UI connects and shows 51.27.

## 3. Pre-flight

- [ ] Actions → **Upgrade holey firmware to 51.292…** opens the wizard. Its "51.292 image SHA-256" is
      `d7171d97f6eef2735db488a001c556fa60e3639033e6ec7c6ef7b18ff0ee8917`, the same as
      `rust/firmware/holey-51.292.sha256`.
- [ ] **Check the controller**: the log viewer shows `Sent: B05`, `Sent: $$`, `Sent: B10 L`, `Sent: B10 R` and
      their replies. The wizard says "Ready to upgrade PCB v1.1 Detected to 51.292" and shows Board
      `PCB v1.1 Detected`, Firmware before `51.27`, and both chain lengths.
- [ ] If it says "The upgrade is blocked", stop here and report the reason: it is the board-hazard check of
      `spec/firmware.md` C1, and a v1.1 board should pass it.

## 4. Rehearsal, part 1: upgrade

- [ ] Tick "The router is switched off…" and press **Upgrade to 51.292**. The wizard walks through "Backing up the
      controller" (avrdude reads the flash back: up to a minute), "Flashing the new firmware" (about 30 s),
      "Waiting for the controller to boot" and "Restoring the settings".
- [ ] It ends at "Motor self-test" with "every setting verified" (or a short list of settings re-sent; note them).
- [ ] On the Pi, `ls ~/.WebControl/firmware-backups/` shows one new directory holding `flash-read.hex`,
      `rollback.hex`, `settings-dump.txt`, `chain-lengths.txt`, `webcontrol.json` and `preflight.txt`. The wizard's
      Backup line names it. Copy it to the Mac: `scp -r pi@maslow.local:.WebControl/firmware-backups ~/webcontrol-backup-m6/`.
- [ ] The log viewer shows, after the flash: the boot banner `PCB v1.1 Detected`, `error: EEPROM read fail`,
      `Firmware Version 51.29`, then one `Sent: $n=...` for every setting **including `$18=12.6000`, `$17=...`,
      `$63=9999` and `$64=-9999`**, then `Sent: $$`, whose reply lists `$18=12.60000000`, then `Sent: G21` (or
      `G20`) and `Sent: G90`. No `$18=126` is ever sent.
- [ ] About shows the firmware build `51.292`, and the Z-limit buttons now appear on the Z-axis screen.

## 5. Rehearsal, part 2: self-test (motors turn)

- [ ] Let the chains go slack (support the sled), tick "The chains are slack…" and press **Run the motor
      self-test**. Each motor turns one way, then the other: L, R, then Z.
- [ ] The wizard lists L, R and Z each with "direction 1 passed, direction 2 passed", and says whether the chain
      lengths match pre-flight. If they differ, it says so: note both values.
- [ ] Jog Z down 1 mm and up 1 mm. It takes as long as on 51.27 (`$18` = 12.6); a Z that moves ten times faster
      means `$18` is 126: press Stop and go to section 8.

## 6. Rehearsal, part 3: roll back

- [ ] Press **Roll back to 51.27**. The wizard shows "Restoring the saved firmware", "Waiting for the controller to
      boot" and "Restoring the settings", then "Rolled back to 51.27", and an alert says the same.
- [ ] The log shows the boot banner `PCB v1.1 Detected` and `Firmware Version 51.27`, then `$$`, the full settings push
      (no `$63`/`$64` this time) and, after the restart its `$38` asks for, the banner again and the push without
      `$38`, then `$$`.
- [ ] Actions → Sync settings: the `$$` reply matches `dollar-dollar-51.27.txt` from step 1 (same values; `$45` may
      differ in the last digits).
- [ ] `wc-switch python`: Python connects, shows "Firmware: Holey 51.27", and raises no EEPROM alert. Jog once in
      each direction. Then `wc-switch rust` again.

## 7. The real upgrade

- [ ] Repeat sections 3, 4 and 5, and stop after the self-test: the controller stays on 51.292.
- [ ] Home, Return to Center and jog in all directions; each move is where it was on 51.27.
- [ ] Optional, with the bit out and Z raised: air-cut a short file you have cut before (needs M4's Play).
- [ ] `wc-switch python`: Python connects to 51.292 and syncs its settings without an EEPROM alert (Python reads
      the shared `webcontrol.json`, which Rust kept up to date). Switch back with `wc-switch rust` if you like.

## 8. If something goes wrong

- The wizard rolls back by itself when the flash fails twice, when the board's banner changes after the flash, or
  when the controller does not answer B05 with 51.29 within 30 s. Wait for "Rolled back" or "The upgrade failed".
- "The upgrade failed" names the backup's `rollback.hex`. Restore it by hand on the Pi, with WebControl stopped:

  ```sh
  wc-switch python && sudo systemctl stop webcontrol.service
  avrdude -patmega2560 -cwiring -P/dev/ttyACM0 -b115200 -D \
    -Uflash:w:$HOME/.WebControl/firmware-backups/<dir>/rollback.hex:i
  sudo systemctl start webcontrol.service
  ```

  Python then connects to 51.27 and pushes its settings (the EEPROM was reset by each flash).
- The rollback image lives only in that webcontrol-rs session: after a restart of webcontrol-rs, the wizard no
  longer offers it, and the command above is the way back.
- If even that fails, the original firmware is the one the Python backend's `buildFirmware.sh` built from madgrizzle/Firmware @ 950fb23
  (`firmware/holey/holey-51.27.hex`; the script is in git history at `220aa0e`);
  flash it the same way.

Report: the boxes that failed, what you saw, the wizard's "Steps so far", `~/.WebControl/webcontrol-rs.log`, the
backup directory, and `journalctl -u webcontrol-rs --since "<the time from step 1>"`.
