# Install and update

What this must show on the real Pi: a release tarball installs and starts, `webcontrol-rs` reports the release's
version everywhere, it comes back after restarts and reboots, and `webcontrol-update` updates, refuses while the
machine is busy, and rolls back. Nothing here moves the machine. Leave the router switched off anyway.

You need: the Pi (`pi@maslow.local`) with the previous release installed, the controller on USB, the draft release's
`webcontrol-rs-<version>-aarch64.tar.gz` and its `.sha256`, and a browser. Read the README's safety and "Before a
run" first. Below, `<version>` is the new release (for example `0.3.0`) and `<previous>` the release before it.

## 1. Install the draft from its tarball

- [ ] On the Pi, before installing: `webcontrol-update --status` prints `Installed: webcontrol-rs <previous>`,
      `Service: active` and `Latest release: v<previous>`. Note the backup line: ______.
- [ ] Copy the tarball and its `.sha256` to the Pi (`scp webcontrol-rs-<version>-aarch64.tar.gz* pi@maslow.local:`).
      On the Pi, `sha256sum -c webcontrol-rs-<version>-aarch64.tar.gz.sha256` prints `OK`.
- [ ] `tar xzf webcontrol-rs-<version>-aarch64.tar.gz`, then
      `bash webcontrol-rs-<version>-aarch64/install-pi.sh webcontrol-rs-<version>-aarch64` prints
      `Installed webcontrol-rs <version> (<commit>, contract <n>) for pi.` and, since the service was running,
      "webcontrol-rs is still running the previous binary, so a job carries on."
- [ ] `sudo systemctl restart webcontrol-rs`. `/opt/webcontrol-rs/webcontrol-rs --version` prints the same line as
      the install, and `<commit>` is the tag's commit (`git rev-parse --short v<version>` on the Mac).
- [ ] `systemctl status webcontrol-rs` shows it active, running as `pi`. `journalctl -u webcontrol-rs -n 20` shows
      `webcontrol-rs listening on 0.0.0.0:5000` and `settings loaded` with `source=File`.
- [ ] `sudo visudo -c` says `parsed OK`, and `sudo cat /etc/sudoers.d/webcontrol-rs` allows only
      `/usr/bin/systemctl poweroff`.
- [ ] `avrdude -? 2>&1 | head -1` prints avrdude's usage (the firmware upgrade needs it).
- [ ] Open `http://maslow.local:5000/`. The version chip in the status bar reads `WebControl <version> · <commit>`,
      and Help ▸ About shows `v<version>`.
- [ ] `curl -s http://maslow.local:5000/api/state | jq '{backend, version}'` shows `"rust"` and `"<version>"`.

## 2. Restarts, reboots and power-off

- [ ] `sudo systemctl restart webcontrol-rs` with the page open: the status bar shows "Server: not connected", then
      "Server: connected" again within a few seconds, without reloading. The controller reconnects too.
- [ ] `sudo reboot`: once the Pi is back, `systemctl is-enabled webcontrol-rs` prints `enabled`, the page reconnects,
      and the controller shows its firmware again.
- [ ] Leave the page open for a minute: it stays connected (the server pings every 25 s).
- [ ] Actions ▸ Turn off Raspberry Pi, and confirm: the Pi powers off (its green LED stops). Power it back on:
      WebControl starts by itself and the page reconnects.

## 3. `webcontrol-update` on the draft: update to the previous release and roll back

A draft is invisible to `webcontrol-update`, so this checks its download, backup, health check and rollback against
the previous published release. Plain `webcontrol-update` is not the way to do this: the latest published release is
older than the draft. `--version` installs it explicitly.

- [ ] The machine is idle: no file running or paused, no tool change waiting.
- [ ] `webcontrol-update --version v<previous>` asks for the `sudo` password once, then prints
      `Updating webcontrol-rs <version> to <previous> from designfrontier/WebControl.`, `Checksum OK.`,
      `Backed up webcontrol-rs <version> to /opt/webcontrol-rs/backup.`, `Restarting webcontrol-rs.`,
      `webcontrol-rs <previous> is running and answering at http://localhost:5000, with the controller connected.`
      and `Updated webcontrol-rs to <previous>.`
- [ ] The open page reconnects by itself, and the version chip now reads `WebControl <previous>`.
- [ ] `webcontrol-update --status` shows `Installed: webcontrol-rs <previous>` and
      `Backup: webcontrol-rs <version>, made <date>`.
- [ ] `webcontrol-update --rollback` prints `Rolling back webcontrol-rs <previous> to <version> ...`, one
      `restored <path>` per file, and `ROLLED BACK: webcontrol-rs <version> is running again, with its settings.`
- [ ] The version chip reads `WebControl <version>` again, and the settings are unchanged (Settings ▸ Maslow shows
      your values, not the defaults).

## 4. The updater refuses while the machine is busy

- [ ] Open a file (any small one, router off, bit out, Z raised), press Play, then Pause. On the Pi,
      `webcontrol-update --version v<previous>` stops at once with
      `webcontrol-update: Not now: a job is running or paused. Finish or stop it, then run webcontrol-update again.`
      Nothing is downloaded and the job stays paused.
- [ ] `webcontrol-update --rollback` refuses the same way. `webcontrol-update --status` still works.
- [ ] Press Stop. Both commands would now run; do not run them.

## 5. After publishing: update the way an owner does

Run this once the draft is published, on the Pi or a spare card with the previous release installed.

- [ ] `webcontrol-update --status` shows `Latest release: v<version>`.
- [ ] `webcontrol-update` (no flags) updates from `<previous>` to `<version>` with the lines of section 3, and the
      version chip follows.
- [ ] `webcontrol-update` again prints
      `webcontrol-rs <version> is installed, the latest published release of designfrontier/WebControl. Nothing to do`
      and changes nothing.
- [ ] `webcontrol-update --rollback` puts `<previous>` back, and `webcontrol-update` brings `<version>` back.

## 6. From the Mac (development builds only)

- [ ] `rust/deploy.sh push` while a job is paused prints "A job is running or paused on pi@maslow.local; stop it
      first." and changes nothing. With the machine idle it copies the
      binary, restarts the service, and the version chip shows the new commit.
- [ ] `rust/deploy.sh rollback` puts the previous binary back (`/opt/webcontrol-rs/webcontrol-rs --version`
      changes back). A tarball install afterwards replaces both.
