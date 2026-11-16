# G-code and board golden fixtures

Outputs recorded from the Python backend's gcode code (`File/gcodeFile.py`, `Connection/firmwareGcode.py`, the
restart half of `Actions/actions.py`, `SerialPortThread`'s line checks) and board code (`Boards/`), in git history up
to `220aa0e`, so the Rust `wc-gcode` crate gives the same answers (`rust/crates/wc-gcode/tests/fixtures.rs`). The
files are frozen: nothing regenerates them. Edit one by hand only for a deliberate behaviour change, in the same
commit as the Rust change, changing only the values the change explains; never paste in what Rust prints.

Hand-edited since: the firmware upper-cases every line before it parses it (`spec/firmware.md` §1), and Python did
not, so where a line is lower case (`g1 x10 y20` in `comments_mixed.nc`, `n60 g1 x20 y10 f500` in
`crlf_numbered.nc`, the lower-case rows of `stream.json`, and the lower-case text an unclosed comment, a stray `)`
or realtime characters leave in a line) its `sanitised` text and the lines built from it are upper case, `m0` pauses, `t3 g0 x1` names tool 3, and the restart states and positions after it count its X, Y and F.

Every file has a `tolerance` (1e-9): the largest absolute difference allowed for a float. Strings, booleans, `null`
and integers must match exactly. Every gcode string (loaded lines, shifted lines, restart lines) is compared exactly.

## `files/*.nc`

The inputs: CAM-like files in mm and inches, arcs and a helix, G91 passes and G91.1, every comment form, CRLF with
line numbers and lower case, lines over 126 characters and long numbers, a units switch, M0/M1/dwell/spindle/laser
words, every T/M6 form, unreadable words, and a 1500-move spiral. `.gitattributes` keeps their line endings.

## `gcode/<file>.json`

Each file is loaded the way the Open G-code route loads it (`loadUpdateFile()` from disk), starting from tolerance
0.5, `absoluteFlag` 0 and draw tool 0.

- `lines`: `data.gcode` with truncate off; `truncated["4"]`, `truncated["2"]`: with truncate on at that many digits.
- `MM` / `INCHES` (the machine's units at load): `zMoves`, `tolerance` after the load, `fileUnits`
  (`gcodeFileUnits` after the preview) and `moves`, the 3D preview (`line3D`) as `gcodeUpdateCompressed` inflates
  to. Python's `color` and `radius` keys are always `null` and are left out.
- `shift[]`: `moveLine` for each `{x, y, truncate}` home shift, as `[line, shifted]` pairs (every line of a short
  file; the first 60 and every 50th after that of a long one).
- `restart[]`: `gcodeStateAt(index)` (`state`) and the lines `queueRestartSequence(state, zTarget, changeTool)`
  queues (`sent`) and its return (`stoppedAtToolChange`), for both machine units, two homes, current tool 0 and 2,
  sampled indices, and `(zTarget, changeTool)` of `(null, true)` (Run from here) and `(1.5, false)` (Resume). An
  `error` names the Python exception instead.
- `position[]`: `findPositionAt(index)` and the feed rate it leaves (`null` when the file set none).

## `gcode/stream.json`

For the first 60 lines of every file plus a table of awkward lines: comment stripping, `_sanitise`, the positioning
mode a line sets, `pausesFirmware`, `changesTool`, `onlySelectsTool`, `toolNamedBy`, `toolTheFirmwareReads`,
`toolChangeLine` for `(nextTool, currentTool)` pairs, and, in G90 (`mode0`) and G91 (`mode1`) with home
41.6763, 60.1447, the line shifted to home and the title of the problem that refuses it.

## `../boards/*.json`

- `cut_grids.json`: `BoardManager.processGCode` cut grids for several files, homes and boards, then again with the
  home moved (the second pass ORs into the first).
- `trim.json`: `trimBoard` on a cut board, in inches and mm.
- `info.json`: `updateBoardInfo` from form text (one refused), and `initializeNewBoard` for three bed sizes.
- `files.json`: loading board files (a legacy convex-hull board, string and integer dimensions, info only), marking
  cuts on them, and the info line and cut data `saveBoard` writes.
