//! Math utilities — mirrors Go `util/mathutil` package.

/// Truncate a float to the specified number of decimal places.
pub fn truncate_decimal_places(val: f64, places: u32) -> f64 {
    let factor = 10_f64.powi(places as i32);
    (val * factor).trunc() / factor
}

/// Round a float to 4 decimal places — mirrors Go `mathutil.RoundTo4Decimals`.
pub fn round_to_4_decimals(amount: f64) -> f64 {
    (amount * 10000.0).round() / 10000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate_decimal_places(3.14159, 2), 3.14);
        assert_eq!(truncate_decimal_places(1.999, 1), 1.9);
    }

    #[test]
    fn test_round_to_4_decimals() {
        assert!((round_to_4_decimals(1.23456) - 1.2346).abs() < f64::EPSILON);
        assert!((round_to_4_decimals(0.0) - 0.0).abs() < f64::EPSILON);
    }
}
