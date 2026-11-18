# The UI: status bar, help, logs, theme, loaded file, layouts

What this must show on real devices: the status bar names the server, the controller, its firmware and the
WebControl build; the help pages and the log viewer work; the display theme follows the setting on every device; the
loaded file's name shows on every screen; and the control screen fits a phone and an iPad in both orientations.
Playwright checks the layouts at the iPad sizes in Chromium; this checks them in Safari on a real iPad, with real
fonts and touch.

Nothing here moves the machine, except where a box starts an air cut. Leave the router switched off and the bit out.

You need: the release installed (`install-update.md` section 1), the controller on USB, a desktop browser, a phone,
and an iPad if you have one. Read the README's safety and "Before a run" first.

## 1. The status bar

- [ ] Open `http://maslow.local:5000/`. The status bar shows "Server: connected", "Controller: /dev/ttyACM0", the
      firmware chip, the version chip and the CPU chip. Hovering "Server" shows "WebControl at <the Pi's address>:5000".
- [ ] The firmware chip reads "Holey 51.27" on 51.27, and "Holey 51.29" on 51.292 (the firmware reports only two
      decimals). Hovering it shows "Controller firmware".
- [ ] The version chip reads `WebControl <version> · <commit>`, the version and commit that
      `/opt/webcontrol-rs/webcontrol-rs --version` prints on the Pi.
- [ ] Unplug the controller's USB cable: "Controller: not connected", the alarm "Connection Failed or Invalid
      Firmware". Plug it back in: it reconnects by itself within 10 s, and the firmware chip shows the
      same version.
- [ ] Make the window narrow, then phone width: the chips shorten (for example "Server", the port, `51.27` or
      `51.29`, the bare commit) but the build commit stays visible at every width.
- [ ] In the browser's developer tools, Network, filter `ws`: one connection to `/ws?ns=%2FMaslowCNC` with status
      101, and no `socket.io` requests.

## 2. Help, About and the log viewer

- [ ] Help ▸ Offline Help opens the help on "WebControl", its screenshot loads, and its links open other pages.
- [ ] Help ▸ Getting Started opens the setup guide.
- [ ] Help ▸ About shows `v<version>`, the platform and the controller firmware (`51.27` or `51.29`).
- [ ] Help ▸ Logs opens `/logs` in a new tab. A second WebSocket, `/ws?ns=%2FMaslowCNCLogs`, opens, and the page says
      "Logging active".
- [ ] The "Log" pane shows the serial lines, `Sent: ...` and the controller's replies, as they happen. The "Abridged
      log" pane shows the same without the `<...>` status reports.
- [ ] `ssh pi@maslow.local tail -5 .WebControl/webcontrol-rs.log` shows the same lines as the Log pane.
- [ ] Actions ▸ Clear log files, and confirm: `~/.WebControl/webcontrol-rs.log` is emptied, then fills again.

## 3. The display theme (Settings ▸ Display)

- [ ] Settings ▸ Display shows "Display theme" with Light, Dark and System, and the current choice ticked.
- [ ] Choose Dark: the page turns dark at once. Open the UI on the phone too: it is dark as well. Choose Light on the
      phone: the desktop page turns light within a second, without a reload.
- [ ] On the Pi, `jq '."WebControl Settings"[] | select(.key == "uiTheme") | .value' ~/.WebControl/webcontrol.json`
      prints `"light"`. No `Sent: $` line appears in the log: the theme is not a controller setting.
- [ ] Reload the desktop page: it paints light from the start, with no dark flash.
- [ ] Choose System: each device follows its own light or dark setting. Switch the phone's dark mode: its page
      follows, and the desktop does not change.
- [ ] Start an air cut (`jobs.md` section 2's setup, or any small file with Z raised), Pause it, and change the theme:
      it applies at once. The job lock refuses the other settings, not this one. Stop the job.
- [ ] Put the theme back the way you like it.

## 4. The loaded file's name

- [ ] With no file loaded, the run panel and the viewer's footer read "File: No file loaded".
- [ ] G-Code ▸ Open a file in a folder: the run panel shows "File:" and its name above Play, Pause and Stop, and the
      viewer's footer shows "File: <name> in <folder>". Hovering the name shows the whole path.
- [ ] With the same page open on the phone, open another file on the desktop: the phone's run panel changes to the new
      name without a reload.
- [ ] G-Code ▸ Clear GCode, and confirm: the name goes from both devices.
- [ ] Upload and open a file with a long name (60 characters or more, no spaces, with dots or dashes): on the phone
      and the iPad the name wraps at a dot or dash, takes at most two lines with an ellipsis, and never pushes the
      buttons off screen.

## 5. Phone and iPad layouts

On the iPad use Safari, and check each box in landscape and in portrait.

- [ ] Load a file with a tool change (for example `jobs.md`'s `aircut.nc`, router off, bit out, Z raised) and Play it,
      press Resume at the M0, and let it run until it stops at the tool change (`T2 M6`). Leave it there for the boxes
      below.
- [ ] Landscape: the menus and the status bar share one header row. Portrait: the header takes two rows.
- [ ] The page does not scroll in either direction. Play, Resume and Stop, the "File:" line, the line pacing control,
      the Job, Line and Complete readouts, the jog arrows and the Controller Messages are all in view at once.
- [ ] Every button is easy to hit with a finger (Playwright checks 44 px). No two controls overlap.
- [ ] The viewer keeps a wide shape; in portrait it takes at most about 40% of the height.
- [ ] Rotate the iPad while the job waits: the layout changes, nothing is lost, and the tool-change prompt is still
      there.
- [ ] With a trackpad or mouse on the iPad, the viewer's cursor readout shows where the pointer is.
- [ ] Press Resume on the iPad, then Stop: both work with a single tap.
- [ ] Phone: the Menu button opens the menus in a drawer, the controls stack in one column, and Play, Pause and Stop
      with the "File:" line stay pinned in view while you scroll.
