# Files, boards, backup and restore

What this must show on the real Pi: G-code files and boards load, preview, edit and save; a 100k-line file loads
within the time budget; and backup, restore, diagnostics and the two settings imports work. Nothing here runs a job.
**The one step that moves the sled is Send Custom GCode in section 2**; keep the router switched off at its own
switch, and the bit out, throughout.

You need: the release installed, the controller connected, the log viewer open, and a browser. Read the README's
"Before a run" first: the backup it makes is the one section 5 restores.

## 1. A 100k-line file and the load time

- [ ] Make the file on the Mac:
      `awk 'BEGIN{print "G21 G90"; for(i=0;i<100000;i++){r=10+i*.004; printf "G1 X%.4f Y%.4f Z-1 F1200\n", r*cos(i*.01), r*sin(i*.01)}}' > ~/big.nc`
      (or use a real job of that size).
- [ ] G-Code ▸ Upload `~/big.nc` into the library's root.
- [ ] G-Code ▸ Open, pick `big.nc`, and time from pressing Open to the spiral drawn in the viewer. Do it twice and
      note the slower time: ______ s. It must be under twice the time the previous release recorded (on a Pi 3B,
      expect a few seconds).
- [ ] While it loads, Play is refused with "Gcode is still loading", and nothing moves.
- [ ] The CPU chip settles back once the preview is drawn.

## 2. Files

- [ ] G-Code ▸ Open lists the library's folders and files, with the last folder and file selected.
- [ ] Jog somewhere off centre and press Set Home Here, then G-Code ▸ View/Edit: the lines are as loaded (the home
      shift applies only when the lines are sent). Actions ▸ Reset home to center afterwards.
- [ ] The G-code index buttons (next and previous line, next and previous Z move, go to line) move the marker on the
      preview, and the line shown matches. View/Edit numbers its lines the same way as Go to line.
- [ ] G-Code ▸ View/Edit: change a line and apply it; View/Edit again shows the change. G-Code ▸ Save it under a new
      name; G-Code ▸ Open lists it.
- [ ] G-Code ▸ Upload a small file into a new folder (created from the upload screen): it opens and draws.
- [ ] G-Code ▸ Clear GCode, and confirm: the preview empties and the run panel says "File: No file loaded".
- [ ] With the router off and the bit out, G-Code ▸ Send Custom GCode `G91`, `G0 X5`, `G90`: the log shows
      `Sent: G91`, `Sent: G0 X5`, `Sent: G90`, the sled moves 5 mm right, and Send Custom GCode shows the text again
      when reopened.
- [ ] Send Custom GCode with a lower-case line, `g0 x-5`: the log shows `Sent: G0 X-5` and the sled moves back
      (`typed_lower_case_g20_switches_units_then_jog`).

## 3. Files from earlier releases

- [ ] A file saved by an earlier WebControl (v0.94 or a previous release) in `~/.WebControl` opens and draws the same
      as it did there.

## 4. Boards

- [ ] Boards ▸ Create/Edit Board: set an ID, a size and the centre, and Save. The board button shows the new ID, and
      the board draws on the preview.
- [ ] Open a G-code file, then Boards ▸ Process GCode: the cut area shows on the board. Boards ▸ Clear Board, and
      confirm: it is gone.
- [ ] Boards ▸ Trim Board on one edge: the size in Create/Edit Board shrinks by that much.
- [ ] Boards ▸ Save Board under a new name, then Boards ▸ Open Board: it is listed and opens.
- [ ] A board saved by an earlier WebControl, if you have one, opens with its cut area.

## 5. Backup, restore, diagnostics and imports

- [ ] Actions ▸ Back up WebControl downloads `wc_backup_<date>-<time>.zip`. On the Pi, `ls ~/wc_backup_*.zip` shows
      at most five.
- [ ] Actions ▸ Download diagnostics file downloads a zip holding `webcontrol.json` and `webcontrol-rs.log`.
- [ ] Change a setting you will notice (Settings ▸ Maslow, Distance Between Motors +1 mm) and delete one of the test
      files from section 2. Then Actions ▸ Restore WebControl… with the backup you just made: WebControl answers
      success, the setting and the file are back, and Actions ▸ Sync settings shows the restored `$2`.
- [ ] Restore a backup made by the previous release, or by v0.94's Backup WebControl if you have one: it answers
      success and its settings show. Then restore this release's backup again.
- [ ] Actions ▸ Import webcontrol.json file, with a `webcontrol.json` whose Serial Connection differs: Settings ▸
      Maslow shows the imported port. Put it back afterwards.
- [ ] Actions ▸ Import groundcontrol.ini file, with one that holds only your current `motorSpacingX` under
      `[Maslow Settings]` and a stale `distPerRot = 70` under `[Computed Settings]`. WebControl recomputes the derived
      settings, so the stale value never reaches the controller: it answers success, and
      `jq '."Computed Settings"[] | select(.key == "distPerRot") | .value' ~/.WebControl/webcontrol.json` prints
      `"63.5"` (gear teeth × chain pitch).
- [ ] Actions ▸ Import webcontrol.json file, with one that holds only
      `{"Computed Settings": [{"key": "distPerRot", "value": "70"}]}`. The same `jq` prints `"63.5"`, and the log
      shows `$13=63.5` before the `$$`, never `$13=70`.

## 6. Clean up

- [ ] Delete `big.nc` and the test files and boards from the UI.
- [ ] Compare `~/.WebControl/webcontrol.json` with the README's backup: only the values you meant to change differ.

Report as the README says.
