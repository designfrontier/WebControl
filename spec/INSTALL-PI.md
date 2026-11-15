# Upgrading the shop Pi from WebControl v0.94 to this build

The machine: Raspberry Pi 3B at `http://192.168.86.155:5000`, controller on `/dev/ttyACM0`, settings in
`~/.WebControl/webcontrol.json`, running the v0.94 release build (`webcontrol-0.94-rpi-singledirectory.tar.gz`).

The new build runs from source in `~/webcontrol-source` with its own Python 3.9 in `.venv`, as the systemd service
`webcontrol.service`. v0.94 stays installed in `~/webcontrol`, untouched, for rollback.

- **Time:** 20–45 minutes the first time, mostly `pip` on the Pi (step 3). v0.94 keeps running during that.
- **Downtime:** about 1 minute, during the switch (step 4).
- **Rollback:** under 2 minutes (step 6).

What carries over and what changes:

- **Settings carry over unchanged.** This build reads `~/.WebControl/webcontrol.json` as it is: the setting keys are
  the same as v0.94's. It also keeps a `webcontrol.json.bak` next to it, which v0.94 ignores. G-code and boards in
  `~/.WebControl` are used as they are.
- **The classic UI is still at `/`.** The new UI is at `/next/`. Both talk to the same server, so you can use either
  one.
- **Re-run holey calibration once.** The solver's library version differs from v0.94's, so the calibration numbers
  can shift slightly. Re-run holey calibration before the next real cut.
- **Leave the controller firmware alone.** A source install carries no firmware `.hex` files, so don't use Upgrade
  Firmware from this build. The firmware already on the controller is fine.

## 0. Check what the Pi runs (read only, 2 minutes)

```sh
ssh pi@192.168.86.155
grep -E '^(PRETTY_NAME|VERSION_CODENAME)' /etc/os-release; python3 --version; uname -m
systemctl cat webcontrol.service
```

What to expect, and what it means:

- **How v0.94 starts:** both the webcontrol-pi SD image and the README's instructions install
  `/etc/systemd/system/webcontrol.service` with `ExecStart=/home/pi/webcontrol/webcontrol` and `User=pi`. If you see
  that, every step below applies as written. If there is no such unit, step 3 reports where WebControl is started
  instead (`/etc/rc.local`, desktop autostart, crontab or a process on port 5000).
- **OS and architecture:** run `dpkg --print-architecture` too. `armhf` is 32-bit Pi OS and `arm64` is 64-bit.
  - **32-bit (`armhf`) works on Bullseye only.** Its Python 3.9 wheels come from piwheels, which builds them on
    Bullseye against Bullseye's libraries. On 32-bit Buster or Bookworm the script stops before changing anything:
    stay on v0.94, or put Bullseye 32-bit or 64-bit Pi OS on a new SD card and keep the old card as the rollback.
  - **64-bit (`arm64`) works on any release** with PyPI's wheels.
- **Python:** `Python 3.9.x` (Raspberry Pi OS 11, Bullseye) is the easy case. 64-bit Buster (3.7) and Bookworm (3.11)
  need `uv`, which downloads its own Python 3.9:
  `curl -LsSf https://astral.sh/uv/install.sh | sh`, then log out and back in.

## 1. Back up (on the Pi)

```sh
cd ~ && tar czf ~/webcontrol-backup-$(date +%F).tar.gz .WebControl webcontrol
```

That holds the settings, G-code and boards (`~/.WebControl`) and the v0.94 install (`~/webcontrol`). Copy it off
the Pi, from the Mac:

```sh
scp 'pi@192.168.86.155:webcontrol-backup-*.tar.gz' ~/Desktop/
```

## 2. Build the new UI on the Mac and copy the repo to the Pi

Node on a Pi 3B is too slow, so `frontend/dist` is built on the Mac and copied over with the source:

```sh
cd ~/projects/me/WebControl
git switch build/pi-packaging        # or master, once this is merged
(cd frontend && npm ci && npm run build)
rsync -az --delete \
  --exclude .git --exclude .venv --exclude node_modules --exclude build --exclude releases --exclude .claude \
  ./ pi@192.168.86.155:webcontrol-source/
```

`--delete` leaves the excluded paths alone on the Pi, so a later re-sync keeps the Pi's own `.venv`.

## 3. Install (on the Pi, while v0.94 keeps running)

```sh
cd ~/webcontrol-source && tools/install_from_source.sh
```

The script runs as `pi` and calls `sudo` itself. It:

1. Stops if `frontend/dist` is missing (redo step 2), or on 32-bit Pi OS other than Bullseye (see step 0).
2. Reports how any existing WebControl is started. Read this part.
3. Installs the system libraries the wheels need with `apt` (on 32-bit Pi OS, the list in `requirements-pi-apt.txt`,
   which OpenCV makes long), creates `.venv` with Python 3.9 (uv if installed, otherwise `python3 -m venv`), and
   installs `requirements.txt`. On 32-bit Pi OS it takes wheels from piwheels and swaps in
   `requirements-pi-overrides.txt`. It then checks that everything imports, and lists any missing system library
   by name.
4. Finds v0.94's `webcontrol.service`, saves a copy of it as `webcontrol-v094.service` (disabled), and stages the new
   unit in `build/webcontrol.service`. **It does not stop, disable or replace v0.94.**

It warns if `pi` is not in the `dialout` group, which it needs to open `/dev/ttyACM0`. It stops if v0.94's unit runs
as a different user than the one running the script, because the settings live in that user's `~/.WebControl`.

## 4. Switch (machine idle)

```sh
cd ~/webcontrol-source && tools/install_from_source.sh --activate
```

This stops and disables v0.94's `webcontrol.service`, installs the new unit under the same name, enables and starts
it, and waits until `/next/` answers. To do the same by hand:

```sh
sudo systemctl disable --now webcontrol
sudo cp ~/webcontrol-source/build/webcontrol.service /etc/systemd/system/webcontrol.service
sudo systemctl daemon-reload
sudo systemctl enable --now webcontrol
```

If v0.94 was not a systemd service, stop it the way step 3 reported it is started (and remove that autostart), then
run `--activate`.

## 5. Verify

1. Open `http://192.168.86.155:5000/`. The classic UI loads and shows the controller connected.
2. Open `http://192.168.86.155:5000/next/`. The status bar shows `Server: connected`, `Controller: /dev/ttyACM0`, and
   the firmware chip shows the controller's firmware (for example `Holey 51.28`), not `Firmware: unknown`.
3. **Dry jog.** Raise the bit clear of the work, or take it out. In `/next/`, jog each direction once with a small
   distance (10 mm) and check that the sled moves the right way by the right amount.
4. **Check Stop.** Jog a long distance (200 mm) and press **Stop** while the sled is moving. It must stop at once.
   Do the same once from the classic UI.
5. Re-run holey calibration before the next real cut.
6. Optional: run `sudo reboot` and check that both UIs come back by themselves.

If something is wrong, see the log with `journalctl -u webcontrol -n 100 --no-pager`, or roll back.

## 6. Roll back to v0.94 (under 2 minutes)

```sh
sudo systemctl disable --now webcontrol && sudo systemctl enable --now webcontrol-v094
```

v0.94 is back at `http://192.168.86.155:5000/` and stays there after a reboot. It reads the same settings file.
To restore the exact pre-upgrade settings as well:

```sh
sudo systemctl stop webcontrol-v094
tar xzf ~/webcontrol-backup-<date>.tar.gz -C ~ .WebControl
sudo systemctl start webcontrol-v094
```

To return to the new build later, run `tools/install_from_source.sh --activate` again. It disables
`webcontrol-v094.service` and starts the new one.

## Updating the new build later

On the Mac, repeat step 2. Then run `tools/install_from_source.sh` on the Pi. It updates `.venv` and restarts
`webcontrol.service` on the new code.

## A PyInstaller release instead (optional)

`tools/build_release.sh` makes `webcontrol-<version>-rpi-singledirectory.tar.gz`, with the same layout as v0.94's
tarball. PyInstaller only builds for the machine it runs on, so this has to run on a Pi or an ARM runner:
`pip install -r requirements.txt -r requirements-build.txt`, then
`tools/build_release.sh --skip-frontend --skip-firmware` with `frontend/dist` copied over as in step 2. The source
install above is simpler, and it is the one this guide covers.
