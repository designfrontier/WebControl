//! The firmware's own triangular inverse (holey 51.27 `Kinematics::_triangularInverse`, and
//! `_oldTriangularInverse` when `$46 ≤ 0`) in float32, as the controller computes it, and the forward solve on it
//! behind the live computed sled position. Checked against `tests/fixtures/firmware32.c`, the same C in float.

use crate::{ChainFeed, Geometry};

#[allow(clippy::approx_constant)]
const FIRMWARE_PI: f32 = 3.14159;
const CHAIN_DENSITY: f32 = 0.14 * 9.8 / 1000.0;
const FORWARD_TOLERANCE: f32 = 0.001;
const FORWARD_GIVE_UP_ERROR: f32 = 0.1;
const MAX_FORWARD_GUESSES: u32 = 5000;

/// The settings the firmware holds as floats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Firmware32 {
    half_width: f32,
    half_height: f32,
    motor_x: f32,
    motor_y: f32,
    r: f32,
    sled_weight: f32,
    chain_elasticity: f32,
    chain_sag_correction: f32,
    left_chain_tolerance: f32,
    right_chain_tolerance: f32,
    rotation_disk_radius: f32,
    feed: ChainFeed,
}

impl From<&Geometry> for Firmware32 {
    fn from(g: &Geometry) -> Self {
        let half_height = g.machine_height as f32 / 2.0;
        Firmware32 {
            half_width: g.machine_width as f32 / 2.0,
            half_height,
            motor_x: g.d as f32 / 2.0,
            motor_y: half_height + g.motor_offset_y as f32,
            r: g.r as f32,
            sled_weight: g.sled_weight as f32,
            chain_elasticity: g.chain_elasticity as f32,
            chain_sag_correction: g.chain_sag_correction as f32,
            left_chain_tolerance: g.left_chain_tolerance as f32,
            right_chain_tolerance: g.right_chain_tolerance as f32,
            rotation_disk_radius: g.rotation_disk_radius as f32,
            feed: g.chain_over_sprocket,
        }
    }
}

/// One chain's start: its distance to the sprocket centre, its angle and the length wrapped on the sprocket.
#[derive(Clone, Copy)]
struct Sprocket {
    distance: f32,
    angle: f32,
    around: f32,
}

fn square(value: f32) -> f32 {
    value * value
}

impl Firmware32 {
    fn sprockets(&self, x: f32, y: f32) -> (Sprocket, Sprocket) {
        let (motor_x, motor_y, r) = (self.motor_x, self.motor_y, self.r);
        let sprocket = |distance: f32| {
            let (rise, wrap) = (((motor_y - y) / distance).asin(), (r / distance).asin());
            let (angle, around) = match self.feed {
                ChainFeed::Top => (rise + wrap, r * (rise + wrap)),
                ChainFeed::Bottom => (rise - wrap, r * (FIRMWARE_PI - (rise - wrap))),
            };
            Sprocket { distance, angle, around }
        };
        (
            sprocket((square(-motor_x - x) + square(motor_y - y)).sqrt()),
            sprocket((square(motor_x - x) + square(motor_y - y)).sqrt()),
        )
    }

    /// Chain lengths for a target, as `forward` calls it: unclamped, except that the old model clamps itself.
    pub fn inverse(&self, x: f32, y: f32) -> (f32, f32) {
        if self.sled_weight <= 0.0 { self.old_inverse(x, y) } else { self.catenary_inverse(x, y) }
    }

    fn catenary_inverse(&self, x: f32, y: f32) -> (f32, f32) {
        let (motor_x, motor_y, r) = (self.motor_x, self.motor_y, self.r);
        let (one, two) = self.sprockets(x, y);
        let ((x1, y1), (x2, y2)) = match self.feed {
            ChainFeed::Top => (
                (-motor_x + r * one.angle.sin(), motor_y + r * one.angle.cos()),
                (motor_x - r * two.angle.sin(), motor_y + r * two.angle.cos()),
            ),
            ChainFeed::Bottom => (
                (-motor_x - r * one.angle.sin(), motor_y - r * one.angle.cos()),
                (motor_x + r * two.angle.sin(), motor_y - r * two.angle.cos()),
            ),
        };
        let straight1 = (square(one.distance) - square(r)).sqrt();
        let straight2 = (square(two.distance) - square(r)).sqrt();
        let weight = self.sled_weight + 0.5 * CHAIN_DENSITY * (straight1 + straight2);
        let denominator = x1 * y2 - x2 * y1 - x1 * y + x * y1 + x2 * y - x * y2;
        let tension1 = -(weight * (square(x1 - x) + square(y1 - y)).sqrt() * (x2 - x)) / denominator;
        let tension2 = (weight * (square(x2 - x) + square(y2 - y)).sqrt() * (x1 - x)) / denominator;
        let a = tension1 * (x - x1) / straight1 / CHAIN_DENSITY;
        let catenary1 = (square(2.0 * a * ((x - x1) / (2.0 * a)).sinh()) + square(y1 - y)).sqrt();
        let catenary2 = (square(2.0 * a * ((x2 - x) / (2.0 * a)).sinh()) + square(y2 - y)).sqrt();
        let chain1 = one.around
            + catenary1 / (1.0 + self.left_chain_tolerance / 100.0) / (1.0 + tension1 * self.chain_elasticity);
        let chain2 = two.around
            + catenary2 / (1.0 + self.right_chain_tolerance / 100.0) / (1.0 + tension2 * self.chain_elasticity);
        (chain1 - self.rotation_disk_radius, chain2 - self.rotation_disk_radius)
    }

    /// The pre-catenary model with its empirical chain-sag term, which clamps the target to the work area itself.
    fn old_inverse(&self, x: f32, y: f32) -> (f32, f32) {
        let clamp = |value: f32, half: f32| {
            if value < -half {
                -half
            } else if value > half {
                half
            } else {
                value
            }
        };
        let (one, two) = self.sprockets(clamp(x, self.half_width), clamp(y, self.half_height));
        let sagged = |own: Sprocket, other: Sprocket| {
            let straight = (square(own.distance) - square(self.r)).sqrt();
            let lean = other.angle.tan() * own.angle.cos() + own.angle.sin();
            straight
                * (1.0
                    + ((self.chain_sag_correction / 1_000_000_000_000.0)
                        * square(own.angle.cos())
                        * square(straight)
                        * square(lean)))
        };
        let chain1 = one.around + sagged(one, two) / (1.0 + self.left_chain_tolerance / 100.0);
        let chain2 = two.around + sagged(two, one) / (1.0 + self.right_chain_tolerance / 100.0);
        (chain1 - self.rotation_disk_radius, chain2 - self.rotation_disk_radius)
    }

    /// `Kinematics::forward`'s float step from `guess`, stopping at 0.001 mm: a tenth of what a `[PE:` report
    /// resolves, and as near as float32 chain lengths of a few metres get. After 5001 steps it settles for 0.1 mm.
    pub fn forward(&self, left: f32, right: f32, guess: (f32, f32)) -> Option<(f32, f32)> {
        let (mut x, mut y) = guess;
        let mut count = 0;
        loop {
            let (guess_left, guess_right) = self.inverse(x, y);
            let (a, b) = (left - guess_left, right - guess_right);
            if !(a.is_finite() && b.is_finite()) {
                return None;
            }
            if a.abs() < FORWARD_TOLERANCE && b.abs() < FORWARD_TOLERANCE {
                return Some((x, y));
            }
            if count > MAX_FORWARD_GUESSES {
                return (a.abs() < FORWARD_GIVE_UP_ERROR && b.abs() < FORWARD_GIVE_UP_ERROR).then_some((x, y));
            }
            x = x + 0.1 * a - 0.1 * b;
            y = y - 0.1 * a - 0.1 * b;
            count += 1;
        }
    }
}
