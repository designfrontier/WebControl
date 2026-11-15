//! `Kinematics.quadrilateralInverse`: a Newton solve for the sled tilt and the chain anchor heights, with the
//! firmware's polynomial sines and cosines. The solver keeps its tilt between calls, as Python's does.

use crate::Geometry;
use crate::checked::{Calc, div, sin, sqrt};

const MAX_ERROR: f64 = 0.01;
const MAX_TRIES: u32 = 10;
const DELTA_PHI: f64 = 0.01;
const DELTA_Y: f64 = 0.01;

#[derive(Debug, Clone, Copy, Default)]
struct Trig {
    sin_phi: f64,
    sin_phi_delta: f64,
    sin_psi1: f64,
    cos_psi1: f64,
    sin_psi2: f64,
    cos_psi2: f64,
    sin_psi1_d: f64,
    cos_psi1_d: f64,
    sin_psi2_d: f64,
    cos_psi2_d: f64,
}

/// The solver's state, carried from one call to the next.
#[derive(Debug, Clone)]
pub struct Solver {
    pub phi: f64,
    x: f64,
    y: f64,
    h: f64,
    theta: f64,
    psi1: f64,
    psi2: f64,
    trig: Trig,
    crit: [f64; 3],
    jac: [f64; 9],
    solution: [f64; 3],
}

impl Default for Solver {
    fn default() -> Self {
        Solver {
            phi: -0.2,
            x: 0.0,
            y: 0.0,
            h: 0.0,
            theta: 0.0,
            psi1: 0.0,
            psi2: 0.0,
            trig: Trig::default(),
            crit: [0.0; 3],
            jac: [0.0; 9],
            solution: [0.0; 3],
        }
    }
}

impl Solver {
    /// `recomputeGeometry`'s part: the chain angles follow the tilt the solver last settled on.
    pub fn recompute(&mut self, g: &Geometry) {
        self.theta = g.theta();
        self.psi1 = self.theta - self.phi;
        self.psi2 = self.theta + self.phi;
    }

    /// Chain lengths for a target already clamped (or deliberately not), NaN when the Jacobian is singular.
    pub fn inverse(&mut self, g: &Geometry, x_target: f64, y_target: f64) -> Calc<(f64, f64)> {
        self.h = g.sled_attach_to_bit();
        self.theta = g.theta();
        self.x = (g.d / 2.0) + x_target;
        self.y = (g.machine_height / 2.0) + g.motor_offset_y - y_target;
        let mirror = self.x > g.d / 2.0;
        if mirror {
            self.x = g.d - self.x;
        }

        let tan_gamma = div(self.y, self.x)?;
        let tan_lambda = div(self.y, g.d - self.x)?;
        let mut y1_plus = g.r * sqrt(1.0 + tan_gamma * tan_gamma)?;
        let mut y2_plus = g.r * sqrt(1.0 + tan_lambda * tan_lambda)?;
        self.my_trig();
        self.criteria(g, y1_plus, y2_plus)?;

        let mut tries = 0;
        while tries <= MAX_TRIES {
            if self.crit.iter().all(|c| c.abs() < MAX_ERROR) {
                break;
            }
            self.jacobian(g, y1_plus, y2_plus)?;
            if !self.mat_solv() {
                return Ok((f64::NAN, f64::NAN));
            }
            self.phi += self.solution[0];
            y1_plus = (y1_plus + self.solution[1]).max_or_nan(g.r);
            y2_plus = (y2_plus + self.solution[2]).max_or_nan(g.r);
            self.psi1 = self.theta - self.phi;
            self.psi2 = self.theta + self.phi;
            self.my_trig();
            self.criteria(g, y1_plus, y2_plus)?;
            tries += 1;
        }
        self.chains(g, y1_plus, y2_plus, mirror)
    }

    fn criteria(&mut self, g: &Geometry, y1_plus: f64, y2_plus: f64) -> Calc<()> {
        let t = self.trig;
        let (phi, h, x) = (self.phi, self.h, self.x);
        self.crit[0] =
            -self.moment(g, y1_plus, y2_plus, phi, t.sin_phi, t.sin_psi1, t.cos_psi1, t.sin_psi2, t.cos_psi2)?;
        self.crit[1] = -self.y_offset(g, y1_plus, x - h * t.cos_psi1, t.sin_psi1)?;
        self.crit[2] = -self.y_offset(g, y2_plus, g.d - (x + h * t.cos_psi2), t.sin_psi2)?;
        Ok(())
    }

    fn jacobian(&mut self, g: &Geometry, y1_plus: f64, y2_plus: f64) -> Calc<()> {
        let t = self.trig;
        let (phi, h, x, crit) = (self.phi, self.h, self.x, self.crit);
        let (s1, c1, s2, c2) = (t.sin_psi1, t.cos_psi1, t.sin_psi2, t.cos_psi2);
        let (s1d, c1d, s2d, c2d) = (t.sin_psi1_d, t.cos_psi1_d, t.sin_psi2_d, t.cos_psi2_d);
        self.jac[0] = (self.moment(g, y1_plus, y2_plus, phi + DELTA_PHI, t.sin_phi_delta, s1d, c1d, s2d, c2d)?
            + crit[0])
            / DELTA_PHI;
        self.jac[1] = (self.moment(g, y1_plus + DELTA_Y, y2_plus, phi, t.sin_phi, s1, c1, s2, c2)? + crit[0]) / DELTA_Y;
        self.jac[2] = (self.moment(g, y1_plus, y2_plus + DELTA_Y, phi, t.sin_phi, s1, c1, s2, c2)? + crit[0]) / DELTA_Y;
        self.jac[3] = (self.y_offset(g, y1_plus, x - h * c1d, s1d)? + crit[1]) / DELTA_PHI;
        self.jac[4] = (self.y_offset(g, y1_plus + DELTA_Y, x - h * c1, s1)? + crit[1]) / DELTA_Y;
        self.jac[5] = 0.0;
        self.jac[6] = (self.y_offset(g, y2_plus, g.d - (x + h * c2d), s2d)? + crit[2]) / DELTA_PHI;
        self.jac[7] = 0.0;
        // Python pairs the tilted cosine with the untilted sine here; kept so the chains match it.
        self.jac[8] = (self.y_offset(g, y2_plus + DELTA_Y, g.d - (x + h * c2d), s2)? + crit[2]) / DELTA_Y;
        Ok(())
    }

    /// `_MatSolv`: Gaussian elimination without pivoting, from the bottom row up, then forward substitution.
    fn mat_solv(&mut self) -> bool {
        const N: usize = 3;
        for i in 1..N {
            let j = N + 1 - i;
            let jj = (j - 1) * N;
            let pivot = self.jac[jj + j - 1];
            if pivot == 0.0 {
                return false;
            }
            for k in 0..j - 1 {
                let kk = k * N;
                let fact = self.jac[kk + j - 1] / pivot;
                (0..j).for_each(|m| self.jac[kk + m] -= fact * self.jac[jj + m]);
                self.crit[k] -= fact * self.crit[j - 1];
            }
        }
        if self.jac[0] == 0.0 {
            return false;
        }
        self.solution[0] = self.crit[0] / self.jac[0];
        for i in 2..=N {
            let row = (i - 1) * N;
            let sum = (0..i - 1).fold(self.crit[i - 1], |sum, j| sum - self.jac[row + j] * self.solution[j]);
            // The elimination already refused a zero on this diagonal.
            self.solution[i - 1] = sum / self.jac[row + i - 1];
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn moment(
        &mut self,
        g: &Geometry,
        y1_plus: f64,
        y2_plus: f64,
        phi: f64,
        sin_phi: f64,
        sin_psi1: f64,
        cos_psi1: f64,
        sin_psi2: f64,
        cos_psi2: f64,
    ) -> Calc<f64> {
        self.psi1 = self.theta - phi;
        self.psi2 = self.theta + phi;
        let h = self.h;
        let tan_gamma = div(self.y - h * sin_psi1 + y1_plus, self.x - h * cos_psi1)?;
        let tan_lambda = div(self.y - h * sin_psi2 + y2_plus, g.d - (self.x + h * cos_psi2))?;
        Ok(g.h3 * sin_phi
            + div(h, tan_lambda + tan_gamma)? * (sin_psi2 - sin_psi1 + (tan_gamma * cos_psi1 - tan_lambda * cos_psi2)))
    }

    fn y_offset(&self, g: &Geometry, y_plus: f64, denominator: f64, psi: f64) -> Calc<f64> {
        Ok(div(sqrt(y_plus * y_plus - g.r * g.r)?, g.r)? - div(self.y + y_plus - self.h * sin(psi)?, denominator)?)
    }

    fn my_trig(&mut self) {
        let (phi, psi1, psi2) = (self.phi, self.psi1, self.psi2);
        let cubic = |c3: f64, c2: f64, c1: f64, c0: f64, v: f64| {
            let square = v * v;
            c3 * (v * square) - c2 * square + c1 * v - c0
        };
        let phi_delta = phi + DELTA_PHI;
        let (psi1_d, psi2_d) = (psi1 - DELTA_PHI, psi2 + DELTA_PHI);
        let sin_phi = |v| cubic(-0.1616, 0.0021, 1.0002, 0.0, v);
        let sin_psi1 = |v| cubic(-0.0942, 0.1368, 1.0965, 0.0241, v);
        let cos_psi1 = |v| cubic(0.1369, 0.6799, 0.1077, -0.9756, v);
        let sin_psi2 = |v| cubic(-0.1460, 0.0197, 1.0068, 0.0008, v);
        let cos_psi2 = |v| cubic(0.0792, 0.5559, 0.0171, -0.9981, v);
        self.trig = Trig {
            sin_phi: sin_phi(phi),
            sin_phi_delta: sin_phi(phi_delta),
            sin_psi1: sin_psi1(psi1),
            cos_psi1: cos_psi1(psi1),
            sin_psi2: sin_psi2(psi2),
            cos_psi2: cos_psi2(psi2),
            sin_psi1_d: sin_psi1(psi1_d),
            cos_psi1_d: cos_psi1(psi1_d),
            sin_psi2_d: sin_psi2(psi2_d),
            cos_psi2_d: cos_psi2(psi2_d),
        };
    }

    fn chains(&self, g: &Geometry, y1_plus: f64, y2_plus: f64, mirror: bool) -> Calc<(f64, f64)> {
        let (x, y, h, t) = (self.x, self.y, self.h, self.trig);
        let (offset_x1, offset_x2) = (h * t.cos_psi1, h * t.cos_psi2);
        let (offset_y1, offset_y2) = (h * t.sin_psi1, h * t.sin_psi2);
        let tan_gamma = div(y - offset_y1 + y1_plus, x - offset_x1)?;
        let tan_lambda = div(y - offset_y2 + y2_plus, g.d - (x + offset_x2))?;
        let near = (x - offset_x1) * (x - offset_x1) + (y + y1_plus - offset_y1) * (y + y1_plus - offset_y1);
        let far_x = g.d - (x + offset_x2);
        let far = far_x * far_x + (y + y2_plus - offset_y2) * (y + y2_plus - offset_y2);
        let near_chain = sqrt(near)? - g.r * tan_gamma + g.r * tan_gamma.atan();
        let far_chain = sqrt(far)? - g.r * tan_lambda + g.r * tan_lambda.atan();
        Ok(if mirror { (far_chain, near_chain) } else { (near_chain, far_chain) })
    }
}

trait MaxOrNan {
    fn max_or_nan(self, floor: f64) -> f64;
}

impl MaxOrNan for f64 {
    /// Python's `if value < floor: value = floor`, which leaves NaN alone where `f64::max` would not.
    fn max_or_nan(self, floor: f64) -> f64 {
        if self < floor { floor } else { self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_singular_jacobian_is_refused() {
        let mut solver = Solver::default();
        assert!(!solver.mat_solv());
        solver.jac = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        assert!(!solver.mat_solv());
    }

    #[test]
    fn a_regular_jacobian_is_solved() {
        let mut solver =
            Solver { jac: [2.0, 0.0, 0.0, 1.0, 4.0, 0.0, 0.0, 1.0, 1.0], crit: [2.0, 6.0, 3.0], ..Solver::default() };
        assert!(solver.mat_solv());
        assert_eq!(solver.solution, [1.0, 1.25, 1.75]);
    }
}
