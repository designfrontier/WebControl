//! Float operations that fail where Python's `math` module raises, so a refusal in Python is a refusal here rather
//! than a NaN or infinity that happens to come out finite later.

/// Python raised `ArithmeticError` or `ValueError` for this calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MathError;

pub type Calc<T> = Result<T, MathError>;

pub fn div(numerator: f64, denominator: f64) -> Calc<f64> {
    if denominator == 0.0 { Err(MathError) } else { Ok(numerator / denominator) }
}

pub fn sqrt(value: f64) -> Calc<f64> {
    if value < 0.0 { Err(MathError) } else { Ok(value.sqrt()) }
}

pub fn asin(value: f64) -> Calc<f64> {
    if value.abs() > 1.0 { Err(MathError) } else { Ok(value.asin()) }
}

/// `math.pow(value, 2)`, which raises on overflow where `value * value` would not.
pub fn square(value: f64) -> Calc<f64> {
    finite_from_finite(value, value * value)
}

pub fn sinh(value: f64) -> Calc<f64> {
    finite_from_finite(value, value.sinh())
}

pub fn sin(value: f64) -> Calc<f64> {
    if value.is_infinite() { Err(MathError) } else { Ok(value.sin()) }
}

fn finite_from_finite(input: f64, output: f64) -> Calc<f64> {
    if input.is_finite() && output.is_infinite() { Err(MathError) } else { Ok(output) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_operation_fails_where_python_raises() {
        assert_eq!(div(1.0, 0.0), Err(MathError));
        assert_eq!(div(1.0, 2.0), Ok(0.5));
        assert_eq!(sqrt(-1.0), Err(MathError));
        assert!(sqrt(f64::NAN).expect("nan passes").is_nan());
        assert_eq!(asin(1.5), Err(MathError));
        assert_eq!(asin(1.0), Ok(1.0f64.asin()));
        assert_eq!(square(1e200), Err(MathError));
        assert_eq!(square(f64::INFINITY), Ok(f64::INFINITY));
        assert_eq!(sinh(1000.0), Err(MathError));
        assert_eq!(sin(f64::INFINITY), Err(MathError));
        assert_eq!(sin(0.0), Ok(0.0));
    }
}
