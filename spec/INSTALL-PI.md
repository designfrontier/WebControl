# Installing WebControl on a Pi, with the old install left on the other disk

This guide puts WebControl (`webcontrol-rs`) on a fresh disk and leaves the existing install untouched on the other
one, so going back is a matter of which disk the Pi boots.

The example is the shop Pi: a Raspberry Pi 3B that boots from a USB SSD with Raspbian 10 Buster 32-bit and WebControl
v0.94 (`webcontrol.service`, `User=pi`, settings in `/home/pi/.WebControl`, at `http://192.168.86.155:5000`).
WebControl cannot run on 32-bit Buster, so it goes on a microSD card with 64-bit Raspberry Pi OS Lite. A Pi 3B with
USB boot enabled tries the SD slot before USB: with the card in, it boots the card, and the SSD stays plugged in and
unchanged. The same steps work the other way round (a new SSD, the old system on a card): swap the two disks' roles.

- **Time:** about 45 minutes, between cuts.
- **Rollback:** power off, pull the card, power on (step 8). v0.94 comes back from the SSD exactly as before.

## 0. What you need

- A 16 GB or larger microSD card, A1 or Class 10, and a USB card reader for the Mac.
- Raspberry Pi Imager 2.0 or newer on the Mac: `brew install --cask raspberry-pi-imager`, or
  `brew upgrade --cask raspberry-pi-imager` if you have it. Older versions cannot set the user, Wi-Fi or SSH on a
  trixie image, so the Pi would boot with no way to log in.
- A WebControl release: `webcontrol-rs-<version>-aarch64.tar.gz` and its `.sha256` from the repository's Releases page,
  or the `webcontrol-rs-<version>-aarch64` artifact of a Release workflow run in Actions (a zip holding the same
  files). Or this repository on the Mac, to build and deploy with `rust/deploy.sh` (see the README's Development
  prerequisites).
- The machine idle: no job running or loaded.

## 1. Back up from the current Pi to the Mac (after the cut)

Run these on the Mac, each in its own terminal window, because `ssh` and `scp` ask for the Pi's password:

```sh
mkdir -p ~/webcontrol-backup
scp -r pi@192.168.86.155:/home/pi/.WebControl ~/webcontrol-backup/
ssh pi@192.168.86.155 systemctl cat webcontrol.service > ~/webcontrol-backup/webcontrol.service.txt
```

`~/webcontrol-backup/.WebControl` now holds the settings (`webcontrol.json`), G-code and boards. The unit file is for
reference only.

Optional, so that ssh stops asking for the password: `ssh-copy-id pi@192.168.86.155`. If you have no key yet, make
one first with `ssh-keygen -t ed25519`. Step 2 puts the same key on the card.

## 2. Flash the card

Put the card in the reader and open Raspberry Pi Imager:

1. **Device:** your Pi (Raspberry Pi 3 for the 3B).
2. **OS:** Raspberry Pi OS (other) → **Raspberry Pi OS Lite (64-bit)** ("A port of Debian Trixie with no desktop
   environment"). If that one gives trouble, **Raspberry Pi OS (Legacy, 64-bit) Lite** (Debian Bookworm) works the
   same way. Not a 32-bit image, and not one with a desktop.
3. **Storage:** the microSD card. Check the size, so you do not pick another disk.
4. **Customisation:**
   - **Hostname:** `maslow`. Step 3 tells the card from the SSD, which may answer to the same name.
   - **User:** `pi`, with a password. The service runs as the user that installs it.
   - **Wi-Fi:** your network name, password and country, unless the Pi uses Ethernet.
   - **Locale:** your time zone and keyboard layout.
   - **SSH:** enabled, with public-key authentication. Paste your key: run `cat ~/.ssh/id_ed25519.pub` on the Mac.
5. Write the card. Imager verifies it and ejects it.

## 3. Boot from the card

1. Shut the old system down: `ssh pi@192.168.86.155 sudo poweroff`. Wait until the green LED stops blinking, then
   unplug the power.
2. Insert the card. Leave the SSD plugged in.
3. Plug the power in. The first boot takes 2–3 minutes and reboots once by itself.
4. On the Mac: `ssh pi@maslow.local`. The Pi's IP address will probably not be 192.168.86.155; use `maslow.local`
   everywhere below. If the Mac cannot find that name, look for `maslow` in your router's device list and use its IP
   address. If only `192.168.86.155:5000` answers, it probably booted the SSD: see below.
5. On the Pi, check that it booted from the card:

   ```sh
   findmnt -n -o SOURCE /
   ```

   It must print `/dev/mmcblk0p2`, the card. `/dev/sda2` means it booted the SSD's Buster.

**If it boots the SSD anyway:** a Pi 3B tries the SD slot first, so it could not boot the card. Power off, push the
card fully in and try again. If that fails, flash the card again and let Imager verify it, or try another card: some
cards do not work with the 3B's boot ROM. As a last resort, unplug the SSD while you use the card, and copy the
settings from the Mac backup in step 4.

## 4. Copy WebControl's folder from the SSD, read-only

On the Pi, find the SSD's root partition:

```sh
lsblk -o NAME,SIZE,FSTYPE,MOUNTPOINTS
```

`mmcblk0` is the card, mounted at `/` and `/boot/firmware`. The SSD is `sda`: `sda1` is its small `vfat` boot
partition and `sda2` its large `ext4` root. If yours differs, use the SSD's disk and `ext4` partition below.

```sh
sudo blockdev --setro /dev/sda
sudo mkdir -p /mnt/ssd
sudo mount -o ro,noload /dev/sda2 /mnt/ssd
cp -a /mnt/ssd/home/pi/.WebControl ~/
sudo umount /mnt/ssd
ls -l ~/.WebControl/webcontrol.json
```

`blockdev --setro` makes the kernel refuse every write to the SSD until the next boot. `ro` mounts it read-only.
`noload` stops ext4 from replaying its journal, which would write to the SSD even on a read-only mount.

Or copy the backup from step 1, from the Mac: `scp -r ~/webcontrol-backup/.WebControl pi@maslow.local:`

WebControl reads v0.94's `webcontrol.json` as it is. Before it first writes the file, it keeps the original once as
`webcontrol.json.pre-rs`.

## 5. Install

Either way below ends with `webcontrol-rs is running and starts at boot: http://maslow.local:5000/`.

**From a release.** Copy the tarball and its `.sha256` to the Pi (`scp webcontrol-rs-<version>-aarch64.tar.gz*
pi@maslow.local:`), then on the Pi:

```sh
sha256sum -c webcontrol-rs-<version>-aarch64.tar.gz.sha256
tar xzf webcontrol-rs-<version>-aarch64.tar.gz
bash webcontrol-rs-<version>-aarch64/install-pi.sh webcontrol-rs-<version>-aarch64
```

**From the Mac**, in this repository: `rust/deploy.sh install`. It builds the UI and the aarch64 binary (with the
51.292 firmware embedded when Docker is available), copies them with the install files and runs the same
`install-pi.sh`.

`install-pi.sh` runs as `pi` and calls `sudo` itself. It:

1. Checks that the Pi is 64-bit ARM and that the binary runs (glibc 2.36 or newer).
2. Installs the binary in `/opt/webcontrol-rs` (keeping the previous one as `webcontrol-rs.prev`), the
   `webcontrol-rs` systemd unit (running as `pi`, in the `dialout` group, on port 5000), `wc-switch`,
   `webcontrol-update` and its `webcontrol-update@.service` unit, and a sudoers rule that lets the service run
   `systemctl poweroff` and start an update or a rollback from the UI, and nothing else.
3. Installs `avrdude` for the firmware upgrade, if it is missing.
4. Enables and starts `webcontrol-rs`. If a `webcontrol.service` from an older WebControl is running on the same disk,
   it leaves that one holding the port and tells you to run `wc-switch rust` when it is idle; one that is enabled but
   stopped it disables, so it cannot take the port at the next boot.

## 6. Verify

1. Open `http://maslow.local:5000/`. The status bar shows `Server: connected` and `Controller: /dev/ttyACM0`.
2. The firmware chip shows the controller's holey firmware (for example `Holey 51.27`), not `Firmware: unknown`, and
   Help → Logs shows the boot banner and `Sent: B05`.
3. Settings → Maslow Settings: Distance Between Motors and the other values are yours, not the defaults. G-Code → Open
   lists your files.
4. **Dry jog.** Raise the bit clear of the work, or take it out. Jog 10 mm, and check that the sled moves the right
   way by the right amount. Then jog 200 mm and press **Stop** while it moves. It must stop at once.
5. If the old install's calibration is more than a few months old, run a fresh holey calibration before the next real
   cut (Actions → Holey Calibration).

If something is wrong: `journalctl -u webcontrol-rs -n 100 --no-pager` and `~/.WebControl/webcontrol-rs.log`.
`spec/rust-checklists/` has the full hardware checklists; its `README.md` says which to run.

## 7. Firmware

Install WebControl before flashing firmware 51.292 to the controller, and flash it from WebControl: Actions → Upgrade
holey firmware to 51.292…. The wizard backs up the controller's flash and settings to
`~/.WebControl/firmware-backups/`, and `spec/rust-checklists/firmware-upgrade.md` walks through the upgrade, the
self-test, the chain lengths and a rollback. Installing first matters: the flash resets the controller's EEPROM, and
WebControl writes your settings back, where an older WebControl could adopt 51.292's defaults (`spec/firmware.md`,
A2). v0.94 was never run against 51.292 and never writes `$18`, so roll the firmware back to 51.27
(`firmware-upgrade.md`, section 7) before going back to v0.94.

## 8. Roll back

If the controller runs 51.292, roll it back to 51.27 first (`spec/rust-checklists/firmware-upgrade.md`, section 7):
v0.94 must not drive 51.292. Then power off (`sudo poweroff`, then wait for the green LED), remove the card, power
on. The Pi boots v0.94 from the SSD at `http://192.168.86.155:5000`, exactly as before. Nothing on the SSD was
changed. To go back to WebControl, put the card back in.

A bad WebControl update needs no disk swap: `webcontrol-update --rollback` on the Pi puts the install before the last
update back (see Updating), and `rust/deploy.sh rollback` from the Mac puts back the binary before the last push.

## 9. Later: making it permanent

Once you are happy with it, move the card's system to the SSD between jobs, never right before a production cut.
Either flash the same OS to the SSD with Imager and repeat steps 3–6 on it, or copy the card to the SSD with
`rpi-clone`. Both replace v0.94 on the SSD, so keep the backup from step 1, and keep the card as the way back.

## Updating

From the browser: Help ▸ Update shows the latest published release with its notes. With the machine idle, press
**Update to vX** and confirm. WebControl downloads it, checks it, backs up this install, installs it and restarts;
the page shows each step, says it is reconnecting while WebControl restarts, and then shows the result (press
**Reload the page** to load the new version's screens). If the new version does not come up, the backup is put back
by itself and the page says so. **Roll back to vY** puts the last backup back the same way. The buttons stay off,
saying why, while a job runs or is paused, a tool change waits, a probe or an optical scan runs, or a firmware upgrade
is underway. An install older than 0.4.0 has no buttons yet: update it once from a terminal as below.

From a terminal, with the machine idle (no job running or paused, no tool change waiting, no probe), run on the Pi
as `pi`:

```sh
webcontrol-update
```

It asks for the `sudo` password once, then installs the latest **published** GitHub Release if it is newer than
the installed one; if the installed one is newer (say, a build pushed from the Mac), it changes nothing. A release
stays a draft until you publish it on GitHub, and drafts are invisible to the updater, so publish the release first.
`webcontrol-update --status` shows the installed version, the backup, the latest published release and whether it is
an update; `--version v0.2.0` installs a particular release, even an older one.

Before it installs, it checks the download's SHA-256 and backs up the current install to
`/opt/webcontrol-rs/backup`: the binary, the systemd unit and drop-in, `wc-switch`, `webcontrol-update`, the sudoers
rule, the update unit and `~/.WebControl/webcontrol.json`. There is one backup; each update replaces the previous one once the new one
is complete. It will not restart a `webcontrol-rs` that is running but not answering, since it may be mid-job, and
if a job starts while it installs it puts the old files back and restarts nothing. After the restart it waits up to
a minute for the service, `/api/state` with the new version and, if one was connected, the controller. If any of
that fails it restores the backup by itself and prints `ROLLED BACK`. To go back by hand, run
`webcontrol-update --rollback` (or **Roll back** in the browser); it keeps the backup.

The backup covers only the last update. The deeper fallback is still the SSD or the old card: power off, boot from
it (step 8), and look at this card later.

Without a published release, install a build with the commands in step 5 and then
`sudo systemctl restart webcontrol-rs` (the install never restarts a running service, so a job in progress carries
on), or run `rust/deploy.sh push` from the Mac, which refuses while a job is running and restarts the service itself.

## Behind a reverse proxy or HTTPS

Not needed for direct use at `http://maslow.local:5000/`. If you ever put WebControl behind a reverse proxy such as
nginx, or behind HTTPS, the proxy must pass the browser's `Host` header through unchanged and forward the WebSocket
upgrade. The server accepts a WebSocket or a change only when its `Origin` matches `Host`, so without these the page
loads but stays disconnected. The name in the browser must also be local (an IP address, `localhost`, a name without
dots, or one ending in `.local`, `.lan`, `.home`, `.home.arpa`, `.internal` or `.localdomain`); a browser using any
other name is refused, even the page itself, so a web page cannot rebind it to the Pi:

```nginx
location / {
    proxy_pass http://127.0.0.1:5000;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
}
```
