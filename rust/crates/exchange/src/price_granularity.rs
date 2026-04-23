/// A single range in a price granularity configuration.
#[derive(Debug, Clone)]
pub struct PriceRange {
    pub min: f64,
    pub max: f64,
    pub increment: f64,
}

/// Finds the appropriate price bucket for the given CPM price.
///
/// Iterates through the sorted `ranges` to find the matching bucket, then
/// rounds down to the nearest increment within that bucket.
///
/// Returns the bucketed price as a formatted string with the given `precision`
/// decimal places. If no range matches, returns `"0"` padded to `precision`.
pub fn get_price_bucket(price: f64, precision: u32, ranges: &[PriceRange]) -> String {
    if price < 0.0 {
        return format_price(0.0, precision);
    }

    for range in ranges {
        if price >= range.max {
            continue;
        }
        if price < range.min {
            break;
        }
        if range.increment <= 0.0 {
            break;
        }
        return get_cpm_target(price, range.min, range.increment, precision);
    }

    format_price(0.0, precision)
}

/// Computes the bucketed CPM string for a price within a specific bucket.
///
/// The price is floored to the nearest `increment` above `bucket_min`, then
/// formatted to `precision` decimal places.
pub fn get_cpm_target(cpm: f64, bucket_min: f64, increment: f64, precision: u32) -> String {
    if increment <= 0.0 {
        return format_price(0.0, precision);
    }

    // Number of full increments above the bucket minimum.
    let steps = ((cpm - bucket_min) / increment).floor() as u64;
    let bucketed = bucket_min + (steps as f64) * increment;

    format_price(bucketed, precision)
}

/// Formats a floating-point price to the given number of decimal places.
fn format_price(value: f64, precision: u32) -> String {
    format!("{:.prec$}", value, prec = precision as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_ranges() -> Vec<PriceRange> {
        // Mimics "medium" granularity: 0-20 by 0.10
        vec![PriceRange {
            min: 0.0,
            max: 20.0,
            increment: 0.10,
        }]
    }

    #[test]
    fn test_get_price_bucket_basic() {
        let ranges = default_ranges();
        assert_eq!(get_price_bucket(1.56, 2, &ranges), "1.50");
    }

    #[test]
    fn test_get_price_bucket_exact() {
        let ranges = default_ranges();
        assert_eq!(get_price_bucket(1.50, 2, &ranges), "1.50");
    }

    #[test]
    fn test_get_price_bucket_zero() {
        let ranges = default_ranges();
        assert_eq!(get_price_bucket(0.0, 2, &ranges), "0.00");
    }

    #[test]
    fn test_get_price_bucket_negative() {
        let ranges = default_ranges();
        assert_eq!(get_price_bucket(-1.0, 2, &ranges), "0.00");
    }

    #[test]
    fn test_get_price_bucket_above_max() {
        let ranges = default_ranges();
        // Price above the max of all ranges returns zero bucket.
        assert_eq!(get_price_bucket(25.0, 2, &ranges), "0.00");
    }

    #[test]
    fn test_get_price_bucket_multiple_ranges() {
        let ranges = vec![
            PriceRange {
                min: 0.0,
                max: 5.0,
                increment: 0.05,
            },
            PriceRange {
                min: 5.0,
                max: 10.0,
                increment: 0.50,
            },
            PriceRange {
                min: 10.0,
                max: 20.0,
                increment: 1.0,
            },
        ];
        assert_eq!(get_price_bucket(3.47, 2, &ranges), "3.45");
        assert_eq!(get_price_bucket(7.83, 2, &ranges), "7.50");
        assert_eq!(get_price_bucket(15.99, 2, &ranges), "15.00");
    }

    #[test]
    fn test_get_cpm_target_basic() {
        assert_eq!(get_cpm_target(1.87, 0.0, 0.10, 2), "1.80");
    }

    #[test]
    fn test_get_cpm_target_zero_increment() {
        assert_eq!(get_cpm_target(1.87, 0.0, 0.0, 2), "0.00");
    }

    #[test]
    fn test_get_cpm_target_precision_zero() {
        assert_eq!(get_cpm_target(1.87, 0.0, 1.0, 0), "1");
    }

    #[test]
    fn test_get_cpm_target_with_offset() {
        assert_eq!(get_cpm_target(6.30, 5.0, 0.50, 2), "6.00");
    }

    #[test]
    fn test_format_price_precision() {
        assert_eq!(format_price(1.5, 2), "1.50");
        assert_eq!(format_price(1.5, 0), "2"); // rounds at 0 precision
        assert_eq!(format_price(0.0, 3), "0.000");
    }

    #[test]
    fn test_get_price_bucket_high_precision() {
        let ranges = default_ranges();
        assert_eq!(get_price_bucket(1.567, 3, &ranges), "1.500");
    }
}
