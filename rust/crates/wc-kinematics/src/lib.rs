//! The kinematics of the holey firmware (spec/firmware.md B14), ported from `Actions/HoleySimulationKinematics.py`:
//! chain lengths for a position (`inverse`) and the position for chain lengths (`forward`), in both modes.
//! Everything is `f64` in Python's operation order, so results match `tests/fixtures/calibration/kinematics.json`.

mod checked;
mod quadrilateral;
mod triangular;

pub use checked::MathError;

/// The firmware's literal for pi in its sprocket radius and chain-wrap maths.
#[allow(clippy::approx_constant)]
pub const FIRMWARE_PI: f64 = 3.14159;

/// N/mm: 0.14 kg/m of chain, hard-coded in the firmware's triangular inverse.
pub const CHAIN_DENSITY: f64 = 0.14 * 9.8 / 1000.0;

const MAX_FORWARD_GUESSES: u32 = 5000;
const FORWARD_TOLERANCE: f64 = 0.000000001;
const FORWARD_GIVE_UP_ERROR: f64 = 0.1;

/// The sprocket radius the firmware derives from `distPerRot` ($13).
pub fn sprocket_radius(dist_per_rot: f64) -> f64 {
    dist_per_rot / (2.0 * FIRMWARE_PI)
}

/// Which side of the sprocket the chain leaves from ($38: 1 is top, anything else bottom).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainFeed {
    Top,
    Bottom,
}

impl ChainFeed {
    pub fn from_setting(value: i64) -> ChainFeed {
        if value == 1 { ChainFeed::Top } else { ChainFeed::Bottom }
    }
}

/// $7: 1 is quadrilateral, anything else triangular.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Triangular,
    Quadrilateral,
}

/// The machine the model describes, in millimetres, newtons and percent; the defaults are Python's class defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub d: f64,
    pub r: f64,
    pub machine_width: f64,
    pub machine_height: f64,
    pub motor_offset_y: f64,
    pub rotation_disk_radius: f64,
    pub sled_weight: f64,
    pub chain_density: f64,
    pub chain_elasticity: f64,
    pub left_chain_tolerance: f64,
    pub right_chain_tolerance: f64,
    pub chain_sag_correction: f64,
    pub chain_over_sprocket: ChainFeed,
    pub l: f64,
    pub s: f64,
    pub h3: f64,
    pub mode: Mode,
}

impl Default for Geometry {
    fn default() -> Self {
        Geometry {
            d: 2978.4,
            r: 10.2,
            machine_width: 2438.4,
            machine_height: 1219.2,
            motor_offset_y: 463.0,
            rotation_disk_radius: 100.0,
            sled_weight: 97.9,
            chain_density: CHAIN_DENSITY,
            chain_elasticity: 5.1685e-6,
            left_chain_tolerance: 0.0,
            right_chain_tolerance: 0.0,
            chain_sag_correction: 0.0,
            chain_over_sprocket: ChainFeed::Top,
            l: 310.0,
            s: 139.0,
            h3: 79.0,
            mode: Mode::Quadrilateral,
        }
    }
}

impl Geometry {
    /// The left sprocket centre is (-x, y) and the right one (x, y), from the centre of the work area.
    fn motor(&self) -> (f64, f64) {
        (self.d / 2.0, self.machine_height / 2.0 + self.motor_offset_y)
    }

    fn sled_attach_to_bit(&self) -> f64 {
        ((self.l / 2.0) * (self.l / 2.0) + self.s * self.s).sqrt()
    }

    fn theta(&self) -> f64 {
        if self.l < 1.0 { 0.0 } else { (2.0 * self.s / self.l).atan() }
    }

    /// Each axis clamped to the work area on its own, as the firmware's `_verifyValidTarget` does.
    pub fn clamp(&self, x: f64, y: f64) -> (f64, f64) {
        let (half_width, half_height) = (self.machine_width / 2.0, self.machine_height / 2.0);
        (clamp_like_python(x, half_width), clamp_like_python(y, half_height))
    }
}

/// `min(max(value, -limit), limit)`, which passes NaN through where `f64::clamp` would not.
fn clamp_like_python(value: f64, limit: f64) -> f64 {
    let floored = if -limit > value { -limit } else { value };
    if limit < floored { limit } else { floored }
}

/// A kinematics model. Quadrilateral solves start from the sled tilt the previous one found, so one model gives
/// the same answers as Python only when it sees the same calls in the same order.
#[derive(Debug, Clone)]
pub struct Kinematics {
    geometry: Geometry,
    solver: quadrilateral::Solver,
}

impl Kinematics {
    pub fn new(geometry: Geometry) -> Kinematics {
        let mut kinematics = Kinematics { geometry, solver: quadrilateral::Solver::default() };
        kinematics.solver.recompute(&kinematics.geometry);
        kinematics
    }

    pub fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    /// Replaces the geometry, as assigning the fields and calling `recomputeGeometry` does in Python.
    pub fn set_geometry(&mut self, geometry: Geometry) {
        self.geometry = geometry;
        self.solver.recompute(&self.geometry);
    }

    /// Chain lengths for a target, clamped to the work area first. `None` where Python raises or returns NaN.
    pub fn inverse(&mut self, x: f64, y: f64) -> Option<(f64, f64)> {
        let (x, y) = self.geometry.clamp(x, y);
        self.inverse_unclamped(x, y)
    }

    pub fn inverse_unclamped(&mut self, x: f64, y: f64) -> Option<(f64, f64)> {
        let chains = match self.geometry.mode {
            Mode::Triangular => triangular::inverse(&self.geometry, x, y),
            Mode::Quadrilateral => self.solver.inverse(&self.geometry, x, y),
        };
        chains.ok().filter(|(left, right)| left.is_finite() && right.is_finite())
    }

    /// The position for chain lengths, starting from Python's default guess of (-10, 10).
    pub fn forward(&mut self, left: f64, right: f64) -> Option<(f64, f64)> {
        self.forward_from(left, right, (-10.0, 10.0))
    }

    /// Steps the guess by a tenth of the chain errors until both are under 1e-9 mm. After 5001 steps it settles for
    /// 0.1 mm, which the quadrilateral solver's own tolerance can need, and otherwise gives `None`.
    pub fn forward_from(&mut self, left: f64, right: f64, guess: (f64, f64)) -> Option<(f64, f64)> {
        let (mut x, mut y) = guess;
        let mut count = 0;
        loop {
            // Unclamped, or every guess past an edge would match the chain lengths of the edge itself.
            let (guess_left, guess_right) = self.inverse_unclamped(x, y)?;
            let (a, b) = (left - guess_left, right - guess_right);
            if !(a.is_finite() && b.is_finite()) {
                return None;
            }
            if a.abs() < FORWARD_TOLERANCE && b.abs() < FORWARD_TOLERANCE {
                return Some((x, y));
            }
            if count > MAX_FORWARD_GUESSES {
                let close = a.abs() < FORWARD_GIVE_UP_ERROR && b.abs() < FORWARD_GIVE_UP_ERROR;
                return close.then_some((x, y));
            }
            x = x + 0.1 * a - 0.1 * b;
            y = y - 0.1 * a - 0.1 * b;
            count += 1;
        }
    }

    /// `updateSetting`: a firmware setting as the stored string. Returns false for a value or key it ignores.
    pub fn update_setting(&mut self, key: u32, value: &str) -> bool {
        let Some(number) = python_float(value) else { return false };
        let g = &mut self.geometry;
        let field = match key {
            0 => Some(&mut g.machine_width),
            1 => Some(&mut g.machine_height),
            2 => Some(&mut g.d),
            3 => Some(&mut g.motor_offset_y),
            8 => Some(&mut g.rotation_disk_radius),
            37 => Some(&mut g.chain_sag_correction),
            40 => Some(&mut g.left_chain_tolerance),
            41 => Some(&mut g.right_chain_tolerance),
            45 => Some(&mut g.chain_elasticity),
            46 => Some(&mut g.sled_weight),
            _ => None,
        };
        match (field, key) {
            (Some(field), _) => *field = number,
            (None, 13) => g.r = sprocket_radius(number),
            (None, 38) if number.is_finite() => g.chain_over_sprocket = ChainFeed::from_setting(number.trunc() as i64),
            (None, 7) if number.is_finite() => {
                g.mode = if number.trunc() == 1.0 { Mode::Quadrilateral } else { Mode::Triangular };
            }
            _ => return false,
        }
        true
    }
}

/// Python's `float(text)` for a stored setting: surrounding whitespace and digit-separating underscores allowed.
pub fn python_float(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let bytes = trimmed.as_bytes();
    let separators_ok = bytes
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'_')
        .all(|(i, _)| i > 0 && bytes[i - 1].is_ascii_digit() && bytes.get(i + 1).is_some_and(u8::is_ascii_digit));
    if !separators_ok {
        return None;
    }
    trimmed.replace('_', "").parse().ok()
}
