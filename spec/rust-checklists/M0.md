# M0 hardware checklist: the Rust skeleton on the microSD card

What M0 must show on the real Pi: `wc-switch rust` serves the same UI over plain WebSockets with the controller shown
as not connected, and `wc-switch python` brings the Python service back. M0 never opens the serial port, so nothing
here can move the machine. Still, leave the router switched off.

You need: the Mac with this branch checked out, the Pi booted from the new microSD card (`pi@maslow.local`, see
`spec/INSTALL-PI.md`), the Python service installed there and working, and a browser.

Tick each box; if one fails, note what you saw and run `wc-switch python`.

## 1. Before you start

- [ ] `ssh pi@maslow.local systemctl is-active webcontrol` prints `active`, and
      `http://maslow.local:5000/` shows the UI with "Controller:" connected or not, as usual.
- [ ] Note the time, so the journal can be read from here: `date`.
- [ ] Back up the settings (never edit the original): `scp -r pi@maslow.local:.WebControl ~/webcontrol-backup-m0/`.

## 2. Build and install (from the Mac, repo root)

- [ ] `pip install ziglang` once if `rust/deploy.sh build` says zig is missing.
- [ ] `rust/deploy.sh build` ends with `built .../webcontrol-rs`.
- [ ] `rust/deploy.sh install` prints `Installed webcontrol-rs 0.1.0 (<commit>, contract 3) for pi.` and, with
      Python's `webcontrol.service` running, "webcontrol.service, an older WebControl, is running and keeps port 5000".
- [ ] Python still runs: `ssh pi@maslow.local systemctl is-active webcontrol webcontrol-rs` prints `active` then
      `inactive`.
- [ ] `ssh pi@maslow.local /opt/webcontrol-rs/webcontrol-rs --version` prints the same commit as
      `git rev-parse --short HEAD`.

## 3. Switch to Rust (on the Pi: `ssh pi@maslow.local`)

- [ ] `wc-switch status` shows `webcontrol active, enabled` and `webcontrol-rs inactive, disabled`, answering
      `python`.
- [ ] `wc-switch rust` prints "WebControl is now the rust backend at http://localhost:5000 (and starts on boot)."
- [ ] `wc-switch status` shows `webcontrol inactive, disabled` and `webcontrol-rs active, enabled`, answering `rust`.
- [ ] `systemctl status webcontrol-rs` shows it running as `pi`; `journalctl -u webcontrol-rs -n 20` shows
      `listening on 0.0.0.0:5000` and `settings loaded` with `source=File`.

## 4. The UI on Rust (browser on the Mac or a phone)

- [ ] `http://maslow.local:5000/` shows the UI, and `http://maslow.local:5000/next/logs` redirects to `/logs`.
- [ ] The status bar shows "Server: connected", "Controller: not connected", "Firmware: unknown". Hovering "Server"
      shows "WebControl at <the Pi's address>:5000".
- [ ] The top bar has no "Classic UI" link: Rust serves no classic UI.
- [ ] No alert pops up on load. (The viewer may say it could not load the preview: `/api/viewer` arrives in M1.)
- [ ] In the browser's developer tools, Network, filter `ws`: one connection to `/ws?ns=%2FMaslowCNC`, status 101, and
      no `socket.io` requests at all.
- [ ] Clicking a control that is enabled (for example a menu action such as Actions → Home) shows an alert
      "Function not currently implemented.. Sorry.[...]", and nothing else happens.
- [ ] Open the log viewer (`/logs`): a second WebSocket to `/ws?ns=%2FMaslowCNCLogs` opens and the page says
      "Logging active". The panes stay empty, as nothing talks to the controller.
- [ ] Help → Help opens the offline help on "WebControl", its screenshot loads, and "First Steps" opens.
- [ ] Leave the page open for a minute: the WebSocket stays connected (the server pings every 25 s).
- [ ] The units and step sizes shown match what Python showed (they come from the same `~/.WebControl/webcontrol.json`).

## 5. Restarts and reconnects

- [ ] `sudo systemctl restart webcontrol-rs`: the page shows "Server: not connected", then "connected" again within a
      few seconds, without reloading the page.
- [ ] `sudo reboot`: after the Pi is back, `wc-switch status` still answers `rust` (enabled is what boots), and the
      page reconnects.
- [ ] Rust did not touch the settings: `ls -l ~/.WebControl` shows no new files, and `webcontrol.json` has the same
      modification time as before the switch.

## 6. Back to Python, with the page left open

- [ ] `wc-switch python` prints "WebControl is now the python backend ...".
- [ ] The page that was open on Rust reloads by itself within about 20 s and then shows the controller and firmware as
      Python reports them, over socket.io (developer tools: `socket.io` requests, no `/ws`).
- [ ] `wc-switch status` shows `webcontrol active, enabled` and `webcontrol-rs inactive, disabled`.
- [ ] Both cannot run at once: `sudo systemctl start webcontrol-rs` stops `webcontrol`
      (`systemctl is-active webcontrol` prints `inactive`); then run `wc-switch python` again.
- [ ] `sudo reboot`: Python comes back as the boot default, and a normal session works (connect, jog, read settings).

## 7. Deploy updates

- [ ] `rust/deploy.sh push` from the Mac while Python is active: it prints "pushed ... restarted only if webcontrol-rs
      was running", and `systemctl is-active webcontrol` is still `active`.
- [ ] `rust/deploy.sh rollback` swaps back to the previous binary (`/opt/webcontrol-rs/webcontrol-rs --version`
      changes).

Report: the boxes that failed, what you saw, and `journalctl -u webcontrol-rs --since "<the time from step 1>"`.
