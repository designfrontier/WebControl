//! `Kinematics.triangularInverse`: chains meeting at the bit, with catenary sag and chain stretch. The operations
//! keep Python's order so the results match it to the last bit or two.

use crate::checked::{Calc, asin, div, sinh, sqrt, square};
use crate::{ChainFeed, FIRMWARE_PI, Geometry};

pub fn inverse(geometry: &Geometry, x: f64, y: f64) -> Calc<(f64, f64)> {
    let g = geometry;
    let (motor_x, motor_y) = g.motor();
    let motor1 = sqrt(square(-motor_x - x)? + square(motor_y - y)?)?;
    let motor2 = sqrt(square(motor_x - x)? + square(motor_y - y)?)?;
    let rise1 = asin(div(motor_y - y, motor1)?)?;
    let rise2 = asin(div(motor_y - y, motor2)?)?;
    let wrap1 = asin(div(g.r, motor1)?)?;
    let wrap2 = asin(div(g.r, motor2)?)?;

    let (around1, around2, tangent1, tangent2) = match g.chain_over_sprocket {
        ChainFeed::Top => {
            let (angle1, angle2) = (rise1 + wrap1, rise2 + wrap2);
            (
                g.r * angle1,
                g.r * angle2,
                (-motor_x + g.r * angle1.sin(), motor_y + g.r * angle1.cos()),
                (motor_x - g.r * angle2.sin(), motor_y + g.r * angle2.cos()),
            )
        }
        ChainFeed::Bottom => {
            let (angle1, angle2) = (rise1 - wrap1, rise2 - wrap2);
            (
                g.r * (FIRMWARE_PI - angle1),
                g.r * (FIRMWARE_PI - angle2),
                (-motor_x - g.r * angle1.sin(), motor_y - g.r * angle1.cos()),
                (motor_x + g.r * angle2.sin(), motor_y - g.r * angle2.cos()),
            )
        }
    };
    let ((x1, y1), (x2, y2)) = (tangent1, tangent2);

    let straight1 = sqrt(square(motor1)? - square(g.r)?)?;
    let straight2 = sqrt(square(motor2)? - square(g.r)?)?;
    let denominator = x1 * y2 - x2 * y1 - x1 * y + x * y1 + x2 * y - x * y2;
    let weight = g.sled_weight + 0.5 * g.chain_density * (straight1 + straight2);
    let tension1 = div(-(weight * sqrt(square(x1 - x)? + square(y1 - y)?)? * (x2 - x)), denominator)?;
    let tension2 = div(weight * sqrt(square(x2 - x)? + square(y2 - y)?)? * (x1 - x), denominator)?;
    let horizontal = div(tension1 * (x - x1), straight1)?;
    let a = div(horizontal, g.chain_density)?;

    let catenary1 = sqrt(square(2.0 * a * sinh(div(x - x1, 2.0 * a)?)?)? + square(y1 - y)?)?;
    let catenary2 = sqrt(square(2.0 * a * sinh(div(x2 - x, 2.0 * a)?)?)? + square(y2 - y)?)?;
    let chain1 =
        around1 + div(div(catenary1, 1.0 + g.left_chain_tolerance / 100.0)?, 1.0 + tension1 * g.chain_elasticity)?;
    let chain2 =
        around2 + div(div(catenary2, 1.0 + g.right_chain_tolerance / 100.0)?, 1.0 + tension2 * g.chain_elasticity)?;
    Ok((chain1 - g.rotation_disk_radius, chain2 - g.rotation_disk_radius))
}
