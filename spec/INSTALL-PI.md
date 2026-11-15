# Trying this WebControl on a microSD card, with v0.94 left on the SSD

The shop Pi is a Raspberry Pi 3B that boots from a USB SSD: Raspbian 10 Buster 32-bit, WebControl v0.94 as
`webcontrol.service` (`ExecStart=/home/pi/webcontrol/webcontrol`, `User=pi`), settings in `/home/pi/.WebControl`,
at `http://192.168.86.155:5000`. This build cannot run on 32-bit Buster, and `tools/install_from_source.sh` refuses
it.

So the new build goes on a microSD card with 64-bit Raspberry Pi OS Lite. A Pi 3B with USB boot enabled tries the
SD slot before USB, so with the card in it boots the card, and the SSD stays plugged in and unchanged.

- **Time:** about 1 hour, between cuts. The install itself (step 6) takes 10–20 minutes on a Pi 3B.
- **Rollback:** power off, pull the card, power on (step 8). v0.94 comes back from the SSD exactly as before.
- **The classic UI is still at `/`.** The new UI is at `/next/`. Both talk to the same server.

`tools/install_from_source.sh --rollback` and `webcontrol-v094.service` are for upgrading v0.94 in place on the same
disk. This guide does not use them: the card has no v0.94 on it, and rolling back means pulling the card.

## 0. What you need

- A 16 GB or larger microSD card, A1 or Class 10.
- A USB card reader for the Mac.
- Raspberry Pi Imager 2.0 or newer on the Mac: `brew install --cask raspberry-pi-imager`, or
  `brew upgrade --cask raspberry-pi-imager` if you have it. Older versions cannot set the user, Wi-Fi or SSH on a
  Trixie image, so the Pi would boot with no way to log in.
- The Pi idle: no job running or loaded.

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

1. **Device:** Raspberry Pi 3.
2. **OS:** Raspberry Pi OS (other) → **Raspberry Pi OS Lite (64-bit)** ("A port of Debian Trixie with no desktop
   environment"). If that one gives trouble, **Raspberry Pi OS (Legacy, 64-bit) Lite** (Debian Bookworm) works the
   same way. Both are tested (`tools/test-install/run.sh`). Not a 32-bit image, and not one with a desktop.
3. **Storage:** the microSD card. Check the size, so you do not pick another disk.
4. **Customisation:** set:
   - **Hostname:** `maslow`. The step 5 check tells the card from the SSD, which may answer to the same name.
   - **User:** `pi`, with a password. The username must be `pi`.
   - **Wi-Fi:** your network name, password and country, unless the Pi uses Ethernet.
   - **Locale:** your time zone and keyboard layout.
   - **SSH:** enabled, with public-key authentication. Paste your key: run `cat ~/.ssh/id_ed25519.pub` on the Mac.
5. Write the card. Imager verifies it and ejects it.

## 3. Boot from the card

1. Shut the old system down: `ssh pi@192.168.86.155 sudo poweroff`. Wait until the green LED stops blinking, then
   unplug the power.
2. Insert the card. Leave the SSD plugged in.
3. Plug the power in. The first boot takes 2–3 minutes and reboots once by itself.
4. On the Mac:

   ```sh
   ssh pi@maslow.local
   ```

   The Pi's IP address will probably not be 192.168.86.155. Use `maslow.local` everywhere below. If the Mac
   cannot find that name, look for `maslow` in your router's device list and use its IP address. If only
   `192.168.86.155:5000` answers, it probably booted the SSD: step 5 tells you.
5. On the Pi, check that it booted from the card:

   ```sh
   findmnt -n -o SOURCE /
   ```

   It must print `/dev/mmcblk0p2`, the card. `/dev/sda2` means it booted the SSD's Buster: see below.

**If it boots the SSD anyway:** a Pi 3B tries the SD slot first, so it booted the SSD because it could not boot the
card. Power off, push the card fully in and try again. If that fails, flash the card again and let Imager verify
it, or try another card: some cards do not work with the 3B's boot ROM. As a last resort, unplug the SSD while you
use the card, and copy the settings from the Mac backup in step 4.

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

Or copy the backup from step 1, from the Mac:

```sh
scp -r ~/webcontrol-backup/.WebControl pi@maslow.local:
```

## 5. Copy the code from the Mac

Node on a Pi 3B is too slow, so the new UI is built on the Mac and copied with the source:

```sh
cd ~/projects/me/WebControl
git switch master
(cd frontend && npm ci && npm run build)
rsync -az --delete \
  --exclude .git --exclude .venv --exclude node_modules --exclude build --exclude releases --exclude .claude \
  --exclude /firmware --exclude /dist \
  ./ pi@maslow.local:webcontrol-source/
```

The script needs `frontend/dist` and stops without it. `/firmware` is left out on purpose (step 9).

## 6. Install

On the Pi:

```sh
cd ~/webcontrol-source && tools/install_from_source.sh && tools/install_from_source.sh --activate
```

It takes 10–20 minutes on a Pi 3B. The script runs as `pi` and calls `sudo` itself. It:

1. Installs uv, which downloads Python 3.9 for WebControl. Raspberry Pi OS has Python 3.13 (Trixie) or 3.11
   (Bookworm). uv's installer prints a note about `PATH`; you can ignore it.
2. Installs a few system packages with `apt`: `libgl1` and `libglib2.0-0` for OpenCV, and `gcc` and `libc6-dev`
   to build psutil.
3. Creates `~/webcontrol-source/.venv`, installs `requirements.txt` and checks that everything imports.
4. Installs `webcontrol.service`, which runs as `pi` and starts at boot, and starts it. `--activate` restarts it
   and waits until `/next/` answers.

It ends with `WebControl is up after …s: http://<ip>:5000/next/`.

## 7. Verify

1. Open `http://maslow.local:5000/next/`. The status bar shows `Server: connected` and
   `Controller: /dev/ttyACM0`.
2. The firmware chip shows the controller's holey firmware version (for example `Holey 51.28`), not
   `Firmware: unknown`, and the firmware line appears in Controller Messages.
3. Open the classic UI at `http://maslow.local:5000/`. It loads and shows the controller connected.
4. Settings → Maslow Settings: Distance Between Motors and the other values are yours, not the defaults.
5. **Dry jog.** Raise the bit clear of the work, or take it out. Jog 10 mm, and check that the sled moves the right
   way by the right amount. Then jog 200 mm and press **Stop** while it moves. It must stop at once.
6. **Run one fresh holey calibration before the next real cut.** The calibration model and `$13` changed in this
   build, so the old numbers are not exact.

If something is wrong, read the log with `journalctl -u webcontrol -n 100 --no-pager`. These lines are normal:
`can't open camera by index` (no camera), `WebSocket transport not available`, and `Trying to connect to controller`
while the controller is unplugged. If you wired buttons or LEDs to the Pi's GPIO pins, check that the log says
`GPIO pin factory:` followed by something other than `MockFactory`; `MockFactory` means GPIO does nothing.

## 8. Roll back

Power off (`sudo poweroff`, then wait for the green LED), remove the card, power on. The Pi boots v0.94 from the
SSD at `http://192.168.86.155:5000`, exactly as before. Nothing on the SSD was changed.

To go back to the new build, put the card back in.

## 9. Later: making it permanent

Once you are happy with the new build, move the card's system to the SSD between jobs, never right before a
production cut. Either flash the same OS to the SSD with Imager and repeat steps 3–7 on it, or copy the card to
the SSD with `rpi-clone`. Both replace v0.94 on the SSD, so keep the backup from step 1.

**Firmware:** install this WebControl before flashing firmware 51.292 to the controller, and do not flash firmware
from this install. Step 5 copies no `firmware/` folder, so Upgrade Firmware stops with "No firmware file found"
before it touches the controller.

## Updating the new build

On the Mac, repeat step 5. Then run `tools/install_from_source.sh` on the Pi. It updates `.venv` and the unit but
leaves the running service alone, so a cut in progress carries on. With the machine idle, run
`tools/install_from_source.sh --activate` to restart it on the new code.

## Testing the install without a Pi

`tools/test-install/run.sh` runs steps 4–6 in Debian Trixie and Bookworm arm64 containers with systemd, then checks
the service, both UIs, `/api/state`, socket.io, the copied settings and a restart. It needs Docker, `jq`, `rsync`
and a built `frontend/dist`.
