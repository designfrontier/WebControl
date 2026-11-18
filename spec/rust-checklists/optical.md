# Cameras and optical calibration (experimental)

What this checklist must show on a real Pi: `webcontrol-rs` opens a camera from the **Camera Source** setting
(Settings ▸ Camera), shows its live picture over the viewer, and closes it again, for each kind of camera a tester may
have: a USB webcam (V4L2) and a Pi Camera Module (through `rpicam-vid`); and then, with the camera on the sled, an
optical scan of the printed target, the geometry fit and its Accept (section 6). The owner has no camera, so nothing
here has run on hardware yet. CI only exercises fake cameras and the synthetic target (`cargo test -p wc-optical`,
`cargo test -p wc-server --test camera`, the `optical_*` transcripts and Playwright's `optical-simulator.spec.ts`).
The camera adapters (`wc-optical/src/camera/{v4l2,rpicam}.rs`) are outside the coverage floor, so this checklist is
their only test. `spec/optical-calibration.md` is the design.

Safety: sections 1 to 5 do not move the machine; leave the router switched off anyway, and keep the controller
disconnected or idle. Section 6 moves the sled across the whole sheet: the README's safety applies there.

You need: the release installed (`install-update.md` section 1), a browser, and one or both cameras. For section 6,
also the controller connected, the machine calibrated, and a printed target. Tick each box; if one fails, note what
you saw, the alert text, and the last lines of `journalctl -u webcontrol-rs -n 50`, then carry on with the next
section.

## 1. Turn the feature on

- [ ] Settings ▸ WebControl: turn on **Optical calibration (experimental)** and save. `GET /api/state`
      (`curl -s http://maslow.local:5000/api/state | jq .experimental`) shows `["optical"]` without a restart.
- [ ] Settings ▸ Camera shows **Camera Source** `auto`, **Video Image Size** `640x480` and **fps** `5`.

## 2. USB webcam (V4L2)

Unplug any Pi Camera Module first, so `auto` cannot pick it.

- [ ] Plug in the webcam. `ls /dev/video*` lists a new node (usually `/dev/video0`; the Pi's own codec and ISP nodes
      are `/dev/video10` and up). Note `v4l2-ctl --list-devices` and `v4l2-ctl -d /dev/video0 --list-formats-ext`
      if `v4l-utils` is installed: which of `YUYV` and `MJPG` it offers at 640x480.
- [ ] Press **Camera** on the control screen (it sends `toggleCamera`; pressed means on). The picture appears within a
      couple of seconds and refreshes about twice a second. It is gray: the overlay shows luma only.
- [ ] Wave a hand in front of the lens: the overlay follows within about a second (the capture drops frames that
      were already waiting, so it never lags by more than one frame).
- [ ] Reload the page: **Camera** still shows the camera is on (the viewer's
      `statusRequest "cameraStatus"` answers `updateCamera on`).
- [ ] Turn the overlay off. Within about 2 s the webcam's LED (if it has one) goes out, and
      `fuser /dev/video0` prints nothing: the camera closes when nobody needs it.
- [ ] Set **Camera Source** to `/dev/video0` explicitly and repeat the on/off. Same result.
- [ ] Set **Video Image Size** to `1024x768` and turn the overlay on. If the webcam supports it, it still streams;
      note `top`'s CPU for `webcontrol-rs` at 640x480 and at 1024x768. Set it back to `640x480`.
- [ ] With the overlay on, unplug the webcam. Within a few seconds an alert says the camera failed (the text names
      `/dev/video0` or "The camera gave no frame") and the overlay turns off. Plug it back in and turn the overlay
      on again: it streams.
- [ ] Set **Camera Source** to `/dev/video9` (a node that does not exist) and turn the overlay on: an alert names
      `/dev/video9` and the overlay turns off.

## 3. Pi Camera Module (rpicam-vid)

Unplug the USB webcam, connect the camera module to the CSI port with the Pi powered off, then boot.

- [ ] `rpicam-hello --list-cameras` lists it (e.g. `0 : imx708 [...]`). If it says "No cameras available!", fix the
      ribbon cable or `/boot/firmware/config.txt` before going on; WebControl cannot do better than this.
- [ ] `rpicam-vid --codec mjpeg --width 640 --height 480 --framerate 5 -t 2000 --nopreview -o /tmp/test.mjpeg`
      exits cleanly and writes a file of some hundreds of KB. Note how long it takes to start.
- [ ] With **Camera Source** `auto`, turn the overlay on: the picture appears (expect a second or two while
      `rpicam-vid` starts) and refreshes about twice a second. `pgrep -a rpicam-vid` shows
      `rpicam-vid --codec mjpeg --width 640 --height 480 --framerate 5 -t 0 --nopreview -o -`.
- [ ] Turn the overlay off: within about 2 s `pgrep rpicam-vid` prints nothing.
- [ ] Set **Camera Source** to `rpicam` and repeat. Same result.
- [ ] With the overlay on, `sudo pkill rpicam-vid`: an alert says `rpicam-vid stopped:` followed by what it printed,
      and the overlay turns off. Turning it on again starts a new `rpicam-vid`.
- [ ] Plug the USB webcam in as well, with **Camera Source** `auto`: the Pi camera wins (`pgrep rpicam-vid` shows
      it running while the overlay is on).

## 4. Other sources and the setting

- [ ] **Camera Source** `webcam` (not a valid value): turning the overlay on alerts "Camera Source "webcam" is not
      auto, rpicam, /dev/videoN, dir:PATH or synthetic" and turns it off.
- [ ] **Camera Source** `synthetic` without `--simulate`: the alert says "The synthetic camera needs webcontrol-rs
      --simulate".
- [ ] With the overlay on, turn **Optical calibration (experimental)** off: the overlay stops getting frames and the
      camera closes (as in 2 and 3). **Camera** now does nothing at all.

## 5. Stop is never held up by the camera

- [ ] Connect the controller (router off). With the overlay on, jog a few mm and press **Stop** at once: the sled
      stops as promptly as with the overlay off. The capture runs on its own thread, never the machine's.

## 6. The scan, the fit and Accept (the sled moves)

Only with the camera mounted on the sled, looking down, and the router off with the bit out.

- [ ] Actions ▸ Optical calibration (experimental) opens, with a note that the feature is experimental. With the
      setting off, the entry is not listed at all.
- [ ] Printable target (Letter) or (A4) opens a page to print. Print it at 100% and check one square's size with a
      ruler against the target's settings: ______ mm.
- [ ] Fix the target flat on the sheet. In the Camera card, **Test image** shows one frame, and **Find centre** finds
      the square under the camera and fills in the optical centre. Save the settings.
- [ ] **Start scan** with a small custom extent first (a few squares). The sled moves square to square; each square
      shows on the heat map, and the progress bar advances. The log shows `G21`, `G90`, `G0 Z<safe height>`, the camera
      check's two 5 mm moves, then for each square `G1 X… Y… F<feed>` followed by `B10 L`, and `G90` at the end
      (`optical_scan_two_cells_mm`).
- [ ] Press **Stop** mid-scan: the sled stops at once, nothing more is queued, and Start scan is enabled again
      (`optical_stop_mid_scan`).
- [ ] Cover the camera during a scan: the square fails, and **Retry square**, **Skip square** and **Stop scan**
      appear. Uncover it and Retry: the scan carries on.
- [ ] Let a scan finish. **Download CSV** downloads the measurements.
- [ ] **Fit geometry** shows fitted Distance Between Motors, Motor Offset Height and the two chain tolerances beside
      the current ones. Note both: ______.
- [ ] **Accept…** asks to confirm; **Write settings** writes them: the log shows the `$2`, `$3`, `$40` and `$41`
      lines, and Settings ▸ Maslow and Advanced show the new values. Put back the values you noted unless you mean to
      keep them.
- [ ] **Clear measurements** empties the heat map.

## 7. What to send back

- Which boxes failed, with the alert text and the `journalctl` lines.
- For each camera: its name (`v4l2-ctl --list-devices` or `rpicam-hello --list-cameras`), the formats it offers,
  and the CPU figures from section 2.
- From section 6: the CSV, the fitted values, and how the scan went (squares failed, time per square).
