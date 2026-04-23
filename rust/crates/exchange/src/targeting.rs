use std::collections::HashMap;

/// Maximum length for targeting key names.
pub const MAX_KEY_LENGTH: usize = 20;

/// Minimum length for targeting key names.
pub const MIN_KEY_LENGTH: usize = 12;

/// Default prefix used in header bidding targeting keys.
pub const DEFAULT_KEY_PREFIX: &str = "hb";

/// Holds the configuration for generating targeting keys on bids.
#[derive(Debug, Clone)]
pub struct TargetData {
    pub price_granularity: PriceGranularity,
    pub media_type_price_granularity: MediaTypePriceGranularity,
    pub include_winners: bool,
    pub include_bidder_keys: bool,
    pub include_cache_bids: bool,
    pub include_cache_vast: bool,
    pub include_format: bool,
    pub prefer_deals: bool,
    pub always_include_deals: bool,
    pub cache_host: String,
    pub cache_path: String,
    pub prefix: String,
}

/// Placeholder for price granularity configuration. The full definition lives in
/// `price_granularity` module; this re-exports a simplified version for use in
/// `TargetData`.
#[derive(Debug, Clone, Default)]
pub struct PriceGranularity {
    pub precision: u32,
    pub ranges: Vec<GranularityRange>,
}

/// A single price granularity range.
#[derive(Debug, Clone)]
pub struct GranularityRange {
    pub min: f64,
    pub max: f64,
    pub increment: f64,
}

/// Per-media-type price granularity overrides.
#[derive(Debug, Clone, Default)]
pub struct MediaTypePriceGranularity {
    pub banner: Option<PriceGranularity>,
    pub video: Option<PriceGranularity>,
    pub native: Option<PriceGranularity>,
}

/// Multi-bid metadata associated with a bidder.
#[derive(Debug, Clone)]
pub struct MultiBidMeta {
    pub max_bids: i32,
    pub target_bidder_code_prefix: String,
}

/// Returns a size string like "300x250". Returns an empty string if either
/// dimension is zero.
pub fn make_hb_size(w: u32, h: u32) -> String {
    if w == 0 || h == 0 {
        String::new()
    } else {
        format!("{}x{}", w, h)
    }
}

/// Looks up multi-bid metadata for the given bidder name. Returns `None` if
/// the bidder is not present in the map.
pub fn get_multi_bid_meta(
    multi_bid_map: &HashMap<String, MultiBidMeta>,
    bidder: &str,
) -> Option<MultiBidMeta> {
    multi_bid_map.get(bidder).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_hb_size_normal() {
        assert_eq!(make_hb_size(300, 250), "300x250");
    }

    #[test]
    fn test_make_hb_size_zero_width() {
        assert_eq!(make_hb_size(0, 250), "");
    }

    #[test]
    fn test_make_hb_size_zero_height() {
        assert_eq!(make_hb_size(300, 0), "");
    }

    #[test]
    fn test_make_hb_size_both_zero() {
        assert_eq!(make_hb_size(0, 0), "");
    }

    #[test]
    fn test_make_hb_size_large() {
        assert_eq!(make_hb_size(1920, 1080), "1920x1080");
    }

    #[test]
    fn test_get_multi_bid_meta_found() {
        let mut map = HashMap::new();
        map.insert(
            "appnexus".to_string(),
            MultiBidMeta {
                max_bids: 3,
                target_bidder_code_prefix: "apn".to_string(),
            },
        );
        let result = get_multi_bid_meta(&map, "appnexus");
        assert!(result.is_some());
        let meta = result.unwrap();
        assert_eq!(meta.max_bids, 3);
        assert_eq!(meta.target_bidder_code_prefix, "apn");
    }

    #[test]
    fn test_get_multi_bid_meta_not_found() {
        let map: HashMap<String, MultiBidMeta> = HashMap::new();
        assert!(get_multi_bid_meta(&map, "unknown").is_none());
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_KEY_LENGTH, 20);
        assert_eq!(MIN_KEY_LENGTH, 12);
        assert_eq!(DEFAULT_KEY_PREFIX, "hb");
    }

    #[test]
    fn test_target_data_defaults() {
        let td = TargetData {
            price_granularity: PriceGranularity::default(),
            media_type_price_granularity: MediaTypePriceGranularity::default(),
            include_winners: true,
            include_bidder_keys: true,
            include_cache_bids: false,
            include_cache_vast: false,
            include_format: false,
            prefer_deals: false,
            always_include_deals: false,
            cache_host: String::new(),
            cache_path: String::new(),
            prefix: DEFAULT_KEY_PREFIX.to_string(),
        };
        assert!(td.include_winners);
        assert!(td.include_bidder_keys);
        assert!(!td.include_cache_bids);
        assert_eq!(td.prefix, "hb");
    }
}
