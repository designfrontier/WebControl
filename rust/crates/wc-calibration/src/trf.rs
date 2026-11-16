//! scipy 1.9's `least_squares(method="trf", jac="2-point", diff_step=0.1, ftol=1e-11)` without bounds, step for
//! step (`trf_no_bounds`, `solve_lsq_trust_region`, `approx_derivative`). Where it stops is an artefact of its
//! rough Jacobian, so only the same algorithm lands on the five-decimal values Python saves.

use nalgebra::{DMatrix, DVector, SVector};

pub(crate) const N: usize = 4;
pub(crate) const M: usize = 12;
pub(crate) type Params = SVector<f64, N>;
pub(crate) type Residuals = SVector<f64, M>;
pub(crate) type Vector = DVector<f64>;
type Matrix = DMatrix<f64>;

const DIFF_STEP: f64 = 0.1;
pub(crate) const FTOL: f64 = 1e-11;
const XTOL: f64 = 1e-8;
pub(crate) const GTOL: f64 = 1e-8;

/// One row of scipy's `verbose=2` table: the state at the top of each outer iteration.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Iteration {
    pub nfev: usize,
    pub cost: f64,
    pub reduction: Option<f64>,
    pub step_norm: Option<f64>,
    pub optimality: f64,
}

/// Where the fit stopped, its residuals there, and the path it took.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fitted {
    pub x: Vector,
    pub f: Vector,
    pub iterations: Vec<Iteration>,
}

/// `_compute_absolute_step` with a relative step: a tenth of the value, or sqrt(eps) where that would not move it.
fn absolute_step(x: f64) -> f64 {
    let sign = if x >= 0.0 { 1.0 } else { -1.0 };
    let step = DIFF_STEP * sign * x.abs();
    if (x + step) - x == 0.0 { f64::EPSILON.sqrt() * sign * x.abs().max(1.0) } else { step }
}

fn jacobian(fun: &impl Fn(&Vector) -> Option<Vector>, x: &Vector, f0: &Vector) -> Option<Matrix> {
    let mut jacobian = Matrix::zeros(f0.len(), x.len());
    (0..x.len()).try_for_each(|i| {
        let mut stepped = x.clone();
        stepped[i] += absolute_step(x[i]);
        let dx = stepped[i] - x[i];
        jacobian.set_column(i, &((fun(&stepped)? - f0) / dx));
        Some(())
    })?;
    Some(jacobian)
}

/// More's trust-region step from one SVD of the Jacobian, and the Levenberg-Marquardt parameter it used.
fn trust_region_step(uf: &Vector, s: &Vector, v: &Matrix, delta: f64, initial_alpha: f64, m: usize) -> (Vector, f64) {
    let suf = s.component_mul(uf);
    let phi_and_derivative = |alpha: f64| {
        let denom = s.map(|s| s * s + alpha);
        let p_norm = suf.component_div(&denom).norm();
        let derivative = -suf.zip_map(&denom, |suf, d| suf * suf / (d * d * d)).sum() / p_norm;
        (p_norm - delta, derivative)
    };
    let full_rank = s[s.len() - 1] > f64::EPSILON * m as f64 * s[0];
    if full_rank {
        let p = -(v * uf.component_div(s));
        if p.norm() <= delta {
            return (p, 0.0);
        }
    }
    let mut alpha_upper = suf.norm() / delta;
    let mut alpha_lower = if full_rank {
        let (phi, derivative) = phi_and_derivative(0.0);
        -phi / derivative
    } else {
        0.0
    };
    let restart = |lower: f64, upper: f64| (0.001 * upper).max((lower * upper).sqrt());
    let mut alpha = if !full_rank && initial_alpha == 0.0 { restart(alpha_lower, alpha_upper) } else { initial_alpha };
    for _ in 0..10 {
        if alpha < alpha_lower || alpha > alpha_upper {
            alpha = restart(alpha_lower, alpha_upper);
        }
        let (phi, derivative) = phi_and_derivative(alpha);
        if phi < 0.0 {
            alpha_upper = alpha;
        }
        let ratio = phi / derivative;
        alpha_lower = alpha_lower.max(alpha - ratio);
        alpha -= (phi + delta) * ratio / delta;
        if phi.abs() < 0.01 * delta {
            break;
        }
    }
    let p = -(v * suf.component_div(&s.map(|s| s * s + alpha)));
    let scale = delta / p.norm();
    (p * scale, alpha)
}

fn update_radius(delta: f64, actual: f64, predicted: f64, step_norm: f64, bound_hit: bool) -> (f64, f64) {
    let ratio = if predicted > 0.0 {
        actual / predicted
    } else if predicted == 0.0 && actual == 0.0 {
        1.0
    } else {
        0.0
    };
    let delta = if ratio < 0.25 {
        0.25 * step_norm
    } else if ratio > 0.75 && bound_hit {
        delta * 2.0
    } else {
        delta
    };
    (delta, ratio)
}

fn terminated(reduction: f64, cost: f64, step_norm: f64, x_norm: f64, ratio: f64) -> bool {
    (reduction < FTOL * cost && ratio > 0.25) || step_norm < XTOL * (XTOL + x_norm)
}

/// Holey calibration's fixed-size problem through `solve`.
pub(crate) fn least_squares(fun: impl Fn(&Params) -> Option<Residuals>, x0: Params) -> Option<Fitted> {
    let fixed =
        |x: &Vector| fun(&Params::from_iterator(x.iter().copied())).map(|f| Vector::from_column_slice(f.as_slice()));
    solve(fixed, Vector::from_column_slice(x0.as_slice()))
}

/// Minimises half the summed squared residuals from `x0`. `None` where scipy (or the residuals) would raise. Only
/// the measurements can make a residual non-finite, and they fail the start, so scipy's retry never arises.
pub(crate) fn solve(fun: impl Fn(&Vector) -> Option<Vector>, x0: Vector) -> Option<Fitted> {
    let max_nfev = 100 * x0.len();
    let mut f = fun(&x0).filter(|f| f.iter().all(|r| r.is_finite()))?;
    let m = f.len();
    let mut x = x0.clone();
    let mut j = jacobian(&fun, &x, &f)?;
    let mut cost = 0.5 * f.dot(&f);
    let mut g = j.transpose() * &f;
    let mut delta = if x0.norm() == 0.0 { 1.0 } else { x0.norm() };
    let (mut alpha, mut nfev, mut done) = (0.0, 1, false);
    let (mut iterations, mut reduction, mut last_step) = (Vec::new(), None, None);
    loop {
        let optimality = g.amax();
        iterations.push(Iteration { nfev, cost, reduction, step_norm: last_step, optimality });
        if optimality < GTOL || done || nfev == max_nfev {
            break;
        }
        let svd = j.clone().svd(true, true);
        let (u, v) = (svd.u?, svd.v_t?.transpose());
        let (s, uf) = (svd.singular_values, u.transpose() * &f);
        let (mut actual, mut trial) = (-1.0, (x.clone(), f.clone(), cost));
        while actual <= 0.0 && nfev < max_nfev {
            let (step, new_alpha) = trust_region_step(&uf, &s, &v, delta, alpha, m);
            alpha = new_alpha;
            let js = &j * &step;
            let predicted = -(0.5 * js.dot(&js) + step.dot(&g));
            let x_new = &x + &step;
            let f_new = fun(&x_new)?;
            nfev += 1;
            let step_norm = step.norm();
            let cost_new = 0.5 * f_new.dot(&f_new);
            actual = cost - cost_new;
            trial = (x_new, f_new, cost_new);
            last_step = Some(step_norm);
            let (delta_new, ratio) = update_radius(delta, actual, predicted, step_norm, step_norm > 0.95 * delta);
            done = terminated(actual, cost, step_norm, x.norm(), ratio);
            if done {
                break;
            }
            alpha *= delta / delta_new;
            delta = delta_new;
        }
        if actual > 0.0 {
            (x, f, cost) = trial;
            j = jacobian(&fun, &x, &f)?;
            g = j.transpose() * &f;
        } else {
            (actual, last_step) = (0.0, Some(0.0));
        }
        reduction = Some(actual);
    }
    Some(Fitted { x, f, iterations })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_relative_except_where_they_would_vanish() {
        assert_eq!(absolute_step(2.0), 0.2);
        assert_eq!(absolute_step(-2.0), -0.2);
        assert_eq!(absolute_step(0.0), f64::EPSILON.sqrt());
        assert_eq!(absolute_step(-5e-324), -f64::EPSILON.sqrt());
    }

    #[test]
    fn the_radius_follows_the_reduction_ratio() {
        assert_eq!(update_radius(1.0, 0.1, 1.0, 0.8, true), (0.2, 0.1));
        assert_eq!(update_radius(1.0, 0.9, 1.0, 1.0, true), (2.0, 0.9));
        assert_eq!(update_radius(1.0, 0.9, 1.0, 0.5, false), (1.0, 0.9));
        assert_eq!(update_radius(1.0, 0.0, 0.0, 0.5, false), (1.0, 1.0));
        assert_eq!(update_radius(1.0, 0.5, -1.0, 0.4, false), (0.1, 0.0));
    }

    #[test]
    fn termination_needs_a_small_good_reduction_or_a_tiny_step() {
        assert!(terminated(1e-13, 1.0, 1.0, 1.0, 0.5));
        assert!(!terminated(1e-13, 1.0, 1.0, 1.0, 0.2));
        assert!(terminated(1.0, 1.0, 1e-17, 0.0, 0.0));
        assert!(!terminated(1.0, 1.0, 1e-3, 0.0, 0.9));
    }

    /// A linear problem: the Gauss-Newton step solves it at once, and a far start goes through the trust region.
    fn linear(x: &Params) -> Option<Residuals> {
        Some(Residuals::from_fn(|i, _| {
            let (a, b) = ((i % N) as f64 + 1.0, (i / N) as f64);
            a * x[i % N] - b - 1.0
        }))
    }

    #[test]
    fn a_linear_problem_is_solved() {
        let fitted = least_squares(linear, Params::zeros()).unwrap();
        let expected = Vector::from_fn(N, |i, _| 2.0 / (i as f64 + 1.0));
        assert!((&fitted.x - &expected).amax() < 1e-6, "{:?}", fitted.x);
        let far = least_squares(linear, Params::repeat(1e4)).unwrap();
        assert!((&far.x - &expected).amax() < 1e-6, "{:?}", far.x);
    }

    #[test]
    fn a_rank_deficient_problem_still_steps_within_the_radius() {
        let flat = |x: &Params| Some(Residuals::from_fn(|i, _| x[0] + x[1] - 3.0 + (i as f64) * 0.0));
        let fitted = least_squares(flat, Params::repeat(50.0)).unwrap();
        assert!((fitted.x[0] + fitted.x[1] - 3.0).abs() < 1e-6, "{:?}", fitted.x);
    }

    #[test]
    fn residuals_that_fail_fail_the_fit() {
        assert_eq!(least_squares(|_| None, Params::zeros()), None);
        let fails_off_start = |x: &Params| if x.amax() == 0.0 { linear(x) } else { None };
        assert_eq!(least_squares(fails_off_start, Params::zeros()), None);
    }
}
