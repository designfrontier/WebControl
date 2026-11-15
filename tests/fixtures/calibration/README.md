# Calibration golden fixtures

Inputs and outputs recorded from the Python calibration code, so a port (the planned Rust one) can check it gives
the same numbers. `tests/test_calibration_fixtures.py` checks that the Python code still reproduces every file;
the case builders live in `tests/calibration_fixture_cases.py`.

All lengths are millimetres. Chain tolerances are percent. Every file that compares floats has a `tolerance` field:
the largest absolute difference allowed for each float in it. A `keyTolerance` object, where present, overrides
`tolerance` for the fields it names, wherever they appear in the file. Integers, strings, booleans and `null` must match
exactly.

## Files

### `holey_calibrate.json`

`HoleyCalibration.Calibrate()`.

- `geometry`: the WebControl settings in force, by section and key, as stored strings (the values of
  `defaultwebcontrol.json`).
- `kinematics`: the parameters of the kinematics model the fit uses. It is the firmware's model: `R` is
  distPerRot / (2 × 3.14159), as the firmware computes it, `chainDensity` is the firmware's hard-coded
  0.14 × 9.8 / 1000 N/mm, and chainElasticity comes from the settings. The sled geometry (`l`, `s`, `h3`)
  keeps the model defaults: the fit is triangular, which does not use it.
- `trueMachine`: the machine used to simulate the "differing machine" measurements.
- `cases[]`: `name`, `measurements` (M1 to M12, in the order of the form; `null` is an empty field) and
  `expected`: `Opt_D`, `Opt_motorOffsetY`, `Opt_leftChainTolerance`, `Opt_rightChainTolerance` (each rounded to 5
  decimals by the code) and `totalError` (root of the summed squared residuals, not rounded). `expected` is `null`
  when the calculation must be refused.
- `keyTolerance`: 1e-3 for the four `Opt_*` fields. They are where a least-squares solver stopped, so another
  correct solver lands up to about 1e-4 away (scipy's `lm`, `dogbox` or a different start all do); `totalError`
  keeps the 1e-6 `tolerance`.

Cases: ideal measurements of the configured geometry; exact measurements of a different machine (the fit must
recover `trueMachine`); four sets with uniform noise of ±0.5 or ±1 mm from fixed seeds, rounded to 0.01 mm; a
missing M5 and a non-numeric M12, both refused.

### `kinematics.json`

`HoleySimulationKinematics.Kinematics` `inverse()` and `forward()`, with `R` = 63.5 / (2 × 3.14159), over a 9 x 9 grid covering the whole default
work area (2438.4 x 1219.2 mm, 304.8 mm by 152.4 mm steps, edges included).

- `modes[]`: one entry for triangular and one for quadrilateral kinematics, each with the model `parameters` and
  `points[]` of `x`, `y`, `left`, `right` (chain lengths from `inverse(x, y)`) and `forward` (`[x, y]` from
  `forward(left, right)` starting at `forwardGuess`, or `null` when it does not converge). `forward` stops once
  both chain errors are under 1e-9 mm; after 5001 guesses it returns its last guess if both errors are under
  0.1 mm, and `null` otherwise.
- `offBed[]`: targets outside the work area and the `chains` `inverse()` gives for them. Each axis is clamped to the
  work area on its own first, as the firmware does.
- `unreachable[]`: chain lengths no position matches; `forward` is always `null`.
- The quadrilateral solver keeps its sled angle between calls, so evaluate on one fresh model, in file order:
  `points` (inverse then forward for each), then `offBed`, then `unreachable`.

### `triangular_calculate.json`

`TriangularCalibration.calculate()`: `cases[]` of `inputs` (the four form fields) and `expected`
(`motorYoffsetEst`, `rotationRadiusEst`, `chainSagCorrectionEst`, `cut34YoffsetEst`), or `null` when the inputs are
refused.

### `accept_commands.json`

Every line each Accept puts on the G-code queue, in order, starting from `geometry`. A firmware setting is
`{command, key, value, units}`, for example `$3=470.0` is key 3, value `"470.0"`, units `"mm"`; units is `"%"` for
the chain tolerances ($40, $41) and `""` for the chain sag correction ($37). Any other line is `{gcode}`.

- `holey`: Accept after calibrating with the exact measurements of `trueMachine`.
- `triangular`: Accept after calculating `inputs`.

## Regenerating

Only regenerate after a deliberate change to the calibration maths, and review the diff:

```sh
.venv/bin/python tests/fixtures/calibration/regenerate.py
```

This runs the fixture test with `REGENERATE_CALIBRATION_FIXTURES=1`, which rewrites the files before checking
them.
