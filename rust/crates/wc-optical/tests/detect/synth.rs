//! A minimal ground-truth renderer for the detection tests: one dark rectangle by exact-ish area coverage, then
//! light gradient, Gaussian blur and noise. Pixel centres sit at integer coordinates, as in `Frame`.

#![allow(dead_code)]

use wc_optical::Frame;

const SUPERSAMPLE: usize = 16;

#[derive(Debug, Clone, Copy)]
pub struct Shot {
    pub width: u32,
    pub height: u32,
    pub centre: (f64, f64),
    pub size_px: (f64, f64),
    /// Counter-clockwise on screen.
    pub angle_deg: f64,
    pub dark: f64,
    pub light: f64,
    pub blur_sigma: f64,
    pub noise_sigma: f64,
    /// Relative brightness change across the frame, left to right and half that top to bottom.
    pub gradient: f64,
    /// A disc of paper colour over the square: centre and radius in px.
    pub occluder: Option<((f64, f64), f64)>,
    pub seed: u64,
}

impl Default for Shot {
    fn default() -> Self {
        Shot {
            width: 320,
            height: 240,
            centre: (159.5, 119.5),
            size_px: (60.0, 60.0),
            angle_deg: 0.0,
            dark: 30.0,
            light: 225.0,
            blur_sigma: 0.0,
            noise_sigma: 0.0,
            gradient: 0.0,
            occluder: None,
            seed: 7,
        }
    }
}

impl Shot {
    /// Signed distance-like value: negative inside the rectangle.
    fn outside(&self, (x, y): (f64, f64)) -> f64 {
        let (sin, cos) = self.angle_deg.to_radians().sin_cos();
        let d = (x - self.centre.0, y - self.centre.1);
        let u = d.0 * cos - d.1 * sin;
        let v = d.0 * sin + d.1 * cos;
        (u.abs() - self.size_px.0 / 2.0).max(v.abs() - self.size_px.1 / 2.0)
    }

    fn occluded(&self, (x, y): (f64, f64)) -> f64 {
        self.occluder.map_or(f64::INFINITY, |((cx, cy), r)| (x - cx).hypot(y - cy) - r)
    }

    fn dark_at(&self, p: (f64, f64)) -> bool {
        self.outside(p) <= 0.0 && self.occluded(p) > 0.0
    }

    fn coverage(&self, p: (f64, f64)) -> f64 {
        if self.outside(p).abs() > 1.0 && self.occluded(p).abs() > 1.0 {
            return if self.dark_at(p) { 1.0 } else { 0.0 };
        }
        let offset = |k: usize| (k as f64 + 0.5) / SUPERSAMPLE as f64 - 0.5;
        let hits = (0..SUPERSAMPLE * SUPERSAMPLE)
            .filter(|n| self.dark_at((p.0 + offset(n % SUPERSAMPLE), p.1 + offset(n / SUPERSAMPLE))))
            .count();
        hits as f64 / (SUPERSAMPLE * SUPERSAMPLE) as f64
    }

    pub fn frame(&self) -> Frame {
        let (w, h) = (self.width as usize, self.height as usize);
        let lit = |x: usize, y: usize| {
            1.0 + self.gradient * ((x as f64 / w as f64 - 0.5) + 0.5 * (y as f64 / h as f64 - 0.5))
        };
        let image = (0..w * h)
            .map(|n| {
                let (x, y) = (n % w, n / w);
                let c = self.coverage((x as f64, y as f64));
                (self.light - (self.light - self.dark) * c) * lit(x, y)
            })
            .collect::<Vec<f64>>();
        let blurred = blur(&blur(&image, w, h, self.blur_sigma, (1, w)), h, w, self.blur_sigma, (w, 1));
        let mut rng = XorShift(self.seed.max(1));
        let luma = blurred.iter().map(|v| (v + self.noise_sigma * rng.gaussian()).round().clamp(0.0, 255.0) as u8);
        Frame { width: self.width, height: self.height, luma: luma.collect() }
    }
}

/// One Gaussian pass along lines of `len` samples, `stride.0` apart, starting `stride.1` apart; edges clamp.
fn blur(image: &[f64], len: usize, lines: usize, sigma: f64, stride: (usize, usize)) -> Vec<f64> {
    if sigma <= 0.0 {
        return image.to_vec();
    }
    let radius = (3.0 * sigma).ceil() as i64;
    let weights = (-radius..=radius).map(|k| (-((k * k) as f64) / (2.0 * sigma * sigma)).exp()).collect::<Vec<_>>();
    let total: f64 = weights.iter().sum();
    let mut out = image.to_vec();
    (0..lines).for_each(|line| {
        (0..len).for_each(|i| {
            out[line * stride.1 + i * stride.0] = (-radius..=radius)
                .map(|k| {
                    let j = (i as i64 + k).clamp(0, len as i64 - 1) as usize;
                    image[line * stride.1 + j * stride.0] * weights[(k + radius) as usize]
                })
                .sum::<f64>()
                / total
        })
    });
    out
}

pub struct XorShift(pub u64);

impl XorShift {
    pub fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }

    pub fn gaussian(&mut self) -> f64 {
        let (u, v) = (self.next_f64(), self.next_f64());
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
    }
}
