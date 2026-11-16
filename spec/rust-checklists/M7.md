# M7 hardware checklist: cameras for optical calibration (experimental)

What this checklist must show on a real Pi: `webcontrol-rs` opens a camera from the **Camera Source** setting
(Settings → Camera Settings), shows its live picture over the viewer, and closes it again, for each kind of camera
a tester may have: a USB webcam (V4L2) and a Pi Camera Module (through `rpicam-vid`). The owner has no camera, so
nothing here has run on hardware yet; CI only exercises fake cameras (`cargo test -p wc-optical --test camera`,
`cargo test -p wc-server --test camera`). The camera adapters (`wc-optical/src/camera/{v4l2,rpicam}.rs`) are
outside the coverage floor, so this checklist is their only test. The scan itself, Accept and the printed target
have their own checklist once they land (`spec/optical-calibration.md`).

Safety: nothing in this checklist moves the machine. Leave the router switched off anyway, and keep the controller
disconnected or idle.

You need: the Pi on Pi OS Lite 64-bit (trixie) with `webcontrol-rs` installed (`rust/README.md`), a browser, and
one or both cameras. Tick each box; if one fails, note what you saw, the alert text, and the last lines of
`journalctl -u webcontrol-rs -n 50`, then carry on with the next section.

## 1. Turn the feature on

- [ ] Settings → WebControl Settings: turn on **Optical calibration (experimental)** and save. `GET /api/state`
      (`curl -s http://maslow.local:5000/api/state | jq .experimental`) shows `["optical"]` without a restart.
- [ ] Settings → Camera Settings shows **Camera Source** `auto`, **Video Image Size** `640x480` and **fps** `5`.

## 2. USB webcam (V4L2)

Unplug any Pi Camera Module first, so `auto` cannot pick it.

- [ ] Plug in the webcam. `ls /dev/video*` lists a new node (usually `/dev/video0`; the Pi's own codec and ISP nodes
      are `/dev/video10` and up). Note `v4l2-ctl --list-devices` and `v4l2-ctl -d /dev/video0 --list-formats-ext`
      if `v4l-utils` is installed: which of `YUYV` and `MJPG` it offers at 640x480.
- [ ] Press **Camera** on the control screen (it sends `toggleCamera`; pressed means on). The picture appears within a
      couple of seconds and refreshes about twice a second. It is gray: the overlay shows luma only.
- [ ] Wave a hand in front of the lens: the overlay follows within about a second (the capture drops frames that
      were already waiting, so it never lags by more than one frame).
- [ ] Reload the page: **Camera** still shows the camera is on (`queryCamera` answers `updateCamera on`).
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

## 6. What to send back

- Which boxes failed, with the alert text and the `journalctl` lines.
- For each camera: its name (`v4l2-ctl --list-devices` or `rpicam-hello --list-cameras`), the formats it offers,
  and the CPU figures from section 2.
