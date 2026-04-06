use std::collections::HashMap;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Targeting keys – standard prebid ad-server targeting key-value pairs
// ---------------------------------------------------------------------------

/// Maximum length of a targeting key (DFP limit).
pub const MAX_KEY_LENGTH: usize = 20;

/// Standard targeting key prefixes produced by Prebid Server.
pub const KEY_BIDDER: &str = "hb_bidder";
pub const KEY_PRICE_BUCKET: &str = "hb_pb";
pub const KEY_SIZE: &str = "hb_size";
pub const KEY_DEAL: &str = "hb_deal";
pub const KEY_CACHE_ID: &str = "hb_cache_id";
pub const KEY_CACHE_HOST: &str = "hb_cache_host";
pub const KEY_CACHE_PATH: &str = "hb_cache_path";
pub const KEY_VAST_CACHE_ID: &str = "hb_uuid";
pub const KEY_FORMAT: &str = "hb_format";
pub const KEY_ENV: &str = "hb_env";
pub const KEY_CACHE_URL: &str = "hb_cache_url";
pub const KEY_CATEGORY_DURATION: &str = "hb_cat_dur";

/// Minimum key length (for custom truncation)
pub const MIN_KEY_LENGTH: usize = 12;

/// Default key prefix
pub const DEFAULT_KEY_PREFIX: &str = "hb";

/// Truncate a targeting key to MAX_KEY_LENGTH.
pub fn truncate_target_key(key: &str) -> String {
    if key.len() <= MAX_KEY_LENGTH {
        key.to_string()
    } else {
        key[..MAX_KEY_LENGTH].to_string()
    }
}

/// Truncate a targeting key to a custom max length.
/// If `max_length` is 0 or less than MIN_KEY_LENGTH, uses MAX_KEY_LENGTH.
pub fn truncate_target_key_custom(key: &str, max_length: usize) -> String {
    let limit = if max_length >= MIN_KEY_LENGTH { max_length } else { MAX_KEY_LENGTH };
    if key.len() <= limit {
        key.to_string()
    } else {
        key[..limit].to_string()
    }
}

/// Build a bidder-suffixed targeting key, e.g. `hb_pb_appnexus`.
pub fn bidder_key(base: &str, bidder: &str) -> String {
    truncate_target_key(&format!("{}_{}", base, bidder))
}

/// Build a bidder-suffixed targeting key with custom max length.
pub fn bidder_key_custom(base: &str, bidder: &str, max_length: usize) -> String {
    truncate_target_key_custom(&format!("{}_{}", base, bidder), max_length)
}

// ---------------------------------------------------------------------------
// Price granularity – buckets CPM prices for targeting
// ---------------------------------------------------------------------------

/// A single range within a price granularity definition.
#[derive(Debug, Clone, Deserialize)]
pub struct GranularityRange {
    pub min: f64,
    pub max: f64,
    pub increment: f64,
}

/// Price granularity configuration controlling how CPMs are bucketed.
#[derive(Debug, Clone)]
pub struct PriceGranularity {
    pub precision: u32,
    pub ranges: Vec<GranularityRange>,
}

impl PriceGranularity {
    /// Low granularity: $0.50 buckets up to $5.
    pub fn low() -> Self {
        Self { precision: 2, ranges: vec![GranularityRange { min: 0.0, max: 5.0, increment: 0.50 }] }
    }
    /// Medium (default): $0.10 buckets up to $20.
    pub fn medium() -> Self {
        Self { precision: 2, ranges: vec![GranularityRange { min: 0.0, max: 20.0, increment: 0.10 }] }
    }
    /// High granularity: $0.01 buckets up to $20.
    pub fn high() -> Self {
        Self { precision: 2, ranges: vec![GranularityRange { min: 0.0, max: 20.0, increment: 0.01 }] }
    }
    /// Auto granularity: multiple tiers.
    pub fn auto() -> Self {
        Self {
            precision: 2,
            ranges: vec![
                GranularityRange { min: 0.0, max: 5.0, increment: 0.05 },
                GranularityRange { min: 5.0, max: 10.0, increment: 0.10 },
                GranularityRange { min: 10.0, max: 20.0, increment: 0.50 },
            ],
        }
    }
    /// Dense granularity: fine-grained low, then coarser.
    pub fn dense() -> Self {
        Self {
            precision: 2,
            ranges: vec![
                GranularityRange { min: 0.0, max: 3.0, increment: 0.01 },
                GranularityRange { min: 3.0, max: 8.0, increment: 0.05 },
                GranularityRange { min: 8.0, max: 20.0, increment: 0.50 },
            ],
        }
    }
    /// Parse a named preset.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "low" => Some(Self::low()),
            "medium" | "med" => Some(Self::medium()),
            "high" => Some(Self::high()),
            "auto" => Some(Self::auto()),
            "dense" => Some(Self::dense()),
            _ => None,
        }
    }
}

impl Default for PriceGranularity {
    fn default() -> Self { Self::medium() }
}

/// Bucket a CPM value according to a price granularity config.
///
/// Returns a string like "1.50" with the configured precision.
pub fn get_price_bucket(cpm: f64, granularity: &PriceGranularity) -> String {
    if cpm < 0.0 {
        return format!("{:.prec$}", 0.0, prec = granularity.precision as usize);
    }
    for range in &granularity.ranges {
        if cpm < range.max && cpm >= range.min {
            if range.increment <= 0.0 {
                return format!("{:.prec$}", 0.0, prec = granularity.precision as usize);
            }
            let buckets = ((cpm - range.min) / range.increment).floor();
            let bucketed = range.min + buckets * range.increment;
            return format!("{:.prec$}", bucketed, prec = granularity.precision as usize);
        }
    }
    // CPM exceeds all ranges – cap at last range max
    if let Some(last) = granularity.ranges.last() {
        format!("{:.prec$}", last.max, prec = granularity.precision as usize)
    } else {
        format!("{:.prec$}", 0.0, prec = granularity.precision as usize)
    }
}

// ---------------------------------------------------------------------------
// Targeting map builder – produces key-value maps for winning bids
// ---------------------------------------------------------------------------

/// Parameters for building targeting key-values.
pub struct TargetingParams<'a> {
    pub bidder: &'a str,
    pub price: f64,
    pub bid_id: &'a str,
    pub imp_id: &'a str,
    pub size: Option<(i32, i32)>,
    pub deal_id: Option<&'a str>,
    pub format: Option<&'a str>,
    pub cache_id: Option<&'a str>,
    pub vast_cache_id: Option<&'a str>,
    pub cache_host: Option<&'a str>,
    pub cache_path: Option<&'a str>,
    pub env: Option<&'a str>,
    pub is_winning_bid: bool,
    /// Category/duration targeting key value (e.g. "cars_30s")
    pub category_duration: Option<&'a str>,
    /// Whether to always include bidder keys for deals regardless of
    /// the include_bidder_keys setting.
    pub always_include_deals: bool,
    /// Whether to include bidder-suffixed keys (default true).
    pub include_bidder_keys: bool,
    /// Whether to include winning (unsuffixed) keys (default true).
    pub include_winners: bool,
}

/// Build the targeting key-value map for a single bid.
///
/// When `is_winning_bid` is true, both the bidder-suffixed and unsuffixed
/// (winning) keys are emitted; otherwise only bidder-suffixed keys are produced.
pub fn make_targeting(params: &TargetingParams, granularity: &PriceGranularity) -> HashMap<String, String> {
    let mut kv = HashMap::new();
    let bucket = get_price_bucket(params.price, granularity);
    let has_deal = params.deal_id.map_or(false, |d| !d.is_empty());

    // Add bidder-suffixed keys when include_bidder_keys is true,
    // or when always_include_deals is true and bid has a deal.
    let add_bidder_keys = params.include_bidder_keys
        || (params.always_include_deals && has_deal);

    if add_bidder_keys {
        kv.insert(bidder_key(KEY_BIDDER, params.bidder), params.bidder.to_string());
        kv.insert(bidder_key(KEY_PRICE_BUCKET, params.bidder), bucket.clone());
        if let Some((w, h)) = params.size {
            kv.insert(bidder_key(KEY_SIZE, params.bidder), format!("{}x{}", w, h));
        }
        if let Some(deal) = params.deal_id {
            if !deal.is_empty() {
                kv.insert(bidder_key(KEY_DEAL, params.bidder), deal.to_string());
            }
        }
        if let Some(cid) = params.cache_id {
            kv.insert(bidder_key(KEY_CACHE_ID, params.bidder), cid.to_string());
        }
        if let Some(vid) = params.vast_cache_id {
            kv.insert(bidder_key(KEY_VAST_CACHE_ID, params.bidder), vid.to_string());
        }
        if let Some(fmt) = params.format {
            kv.insert(bidder_key(KEY_FORMAT, params.bidder), fmt.to_string());
        }
        if let Some(env) = params.env {
            kv.insert(bidder_key(KEY_ENV, params.bidder), env.to_string());
        }
        if let Some(host) = params.cache_host {
            kv.insert(bidder_key(KEY_CACHE_HOST, params.bidder), host.to_string());
        }
        if let Some(path) = params.cache_path {
            kv.insert(bidder_key(KEY_CACHE_PATH, params.bidder), path.to_string());
        }
        if let Some(cat_dur) = params.category_duration {
            if !cat_dur.is_empty() {
                kv.insert(bidder_key(KEY_CATEGORY_DURATION, params.bidder), cat_dur.to_string());
            }
        }
    }

    // Add unsuffixed (winning) keys
    if params.include_winners && params.is_winning_bid {
        kv.insert(KEY_BIDDER.to_string(), params.bidder.to_string());
        kv.insert(KEY_PRICE_BUCKET.to_string(), bucket);
        if let Some((w, h)) = params.size {
            kv.insert(KEY_SIZE.to_string(), format!("{}x{}", w, h));
        }
        if let Some(deal) = params.deal_id {
            if !deal.is_empty() {
                kv.insert(KEY_DEAL.to_string(), deal.to_string());
            }
        }
        if let Some(cid) = params.cache_id {
            kv.insert(KEY_CACHE_ID.to_string(), cid.to_string());
        }
        if let Some(vid) = params.vast_cache_id {
            kv.insert(KEY_VAST_CACHE_ID.to_string(), vid.to_string());
        }
        if let Some(fmt) = params.format {
            kv.insert(KEY_FORMAT.to_string(), fmt.to_string());
        }
        if let Some(env) = params.env {
            kv.insert(KEY_ENV.to_string(), env.to_string());
        }
        if let Some(host) = params.cache_host {
            kv.insert(KEY_CACHE_HOST.to_string(), host.to_string());
        }
        if let Some(path) = params.cache_path {
            kv.insert(KEY_CACHE_PATH.to_string(), path.to_string());
        }
        if let Some(cat_dur) = params.category_duration {
            if !cat_dur.is_empty() {
                kv.insert(KEY_CATEGORY_DURATION.to_string(), cat_dur.to_string());
            }
        }
    }

    kv
}

// ---------------------------------------------------------------------------
// Ad-server targeting rules (custom key extraction from ext.prebid)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct AdServerTargetingRule {
    pub key: String,
    pub source: String,  // "bidrequest", "bidresponse", "static"
    pub value: String,   // JSONPath-like: "site.page", "seatbid.0.bid.0.price", or literal
}

/// Apply ad server targeting rules from req.ext.prebid.adservertargeting.
pub fn apply_adserver_targeting(
    rules: &[AdServerTargetingRule],
    bid_request: &openrtb::BidRequest,
    bid: Option<&openrtb::Bid>,
) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for rule in rules {
        let value = match rule.source.as_str() {
            "static" => Some(rule.value.clone()),
            "bidrequest" => extract_from_request(bid_request, &rule.value),
            "bidresponse" => bid.and_then(|b| extract_from_bid(b, &rule.value)),
            _ => None,
        };
        if let Some(v) = value {
            result.insert(rule.key.clone(), v);
        }
    }
    result
}

fn extract_from_request(req: &openrtb::BidRequest, path: &str) -> Option<String> {
    match path {
        "site.page" => req.site.as_ref()?.page.clone(),
        "site.domain" => req.site.as_ref()?.domain.clone(),
        "app.bundle" => req.app.as_ref()?.bundle.clone(),
        "user.id" => req.user.as_ref()?.id.clone(),
        _ => None,
    }
}

fn extract_from_bid(bid: &openrtb::Bid, path: &str) -> Option<String> {
    match path {
        "price" => Some(format!("{}", bid.price)),
        "id" => Some(bid.id.clone()),
        "adid" => bid.adid.clone(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_key() {
        assert_eq!(truncate_target_key("hb_pb"), "hb_pb");
        assert_eq!(
            truncate_target_key("hb_pb_very_long_bidder_name"),
            "hb_pb_very_long_bidd"
        );
    }

    #[test]
    fn test_bidder_key() {
        assert_eq!(bidder_key("hb_pb", "appnexus"), "hb_pb_appnexus");
        assert_eq!(bidder_key("hb_bidder", "rubicon").len(), 17);
    }

    #[test]
    fn test_price_bucket_medium() {
        let g = PriceGranularity::medium();
        assert_eq!(get_price_bucket(0.0, &g), "0.00");
        assert_eq!(get_price_bucket(1.55, &g), "1.50");
        assert_eq!(get_price_bucket(19.99, &g), "19.90");
        assert_eq!(get_price_bucket(25.0, &g), "20.00"); // capped
    }

    #[test]
    fn test_price_bucket_low() {
        let g = PriceGranularity::low();
        assert_eq!(get_price_bucket(1.25, &g), "1.00");
        assert_eq!(get_price_bucket(4.99, &g), "4.50");
        assert_eq!(get_price_bucket(6.0, &g), "5.00"); // capped
    }

    #[test]
    fn test_price_bucket_high() {
        let g = PriceGranularity::high();
        assert_eq!(get_price_bucket(1.234, &g), "1.23");
        assert_eq!(get_price_bucket(0.01, &g), "0.01");
    }

    #[test]
    fn test_price_bucket_auto() {
        let g = PriceGranularity::auto();
        assert_eq!(get_price_bucket(2.57, &g), "2.55");  // $0.05 inc below $5
        assert_eq!(get_price_bucket(7.34, &g), "7.30");  // $0.10 inc $5-$10
        assert_eq!(get_price_bucket(15.7, &g), "15.50"); // $0.50 inc $10-$20
    }

    #[test]
    fn test_price_bucket_dense() {
        let g = PriceGranularity::dense();
        assert_eq!(get_price_bucket(1.234, &g), "1.23");  // $0.01 inc below $3
        assert_eq!(get_price_bucket(5.07, &g), "5.05");   // $0.05 inc $3-$8
        assert_eq!(get_price_bucket(12.3, &g), "12.00");  // $0.50 inc $8-$20
    }

    #[test]
    fn test_price_bucket_negative() {
        let g = PriceGranularity::medium();
        assert_eq!(get_price_bucket(-1.0, &g), "0.00");
    }

    #[test]
    fn test_price_bucket_from_name() {
        assert!(PriceGranularity::from_name("low").is_some());
        assert!(PriceGranularity::from_name("med").is_some());
        assert!(PriceGranularity::from_name("MEDIUM").is_some());
        assert!(PriceGranularity::from_name("unknown").is_none());
    }

    #[test]
    fn test_make_targeting_winning() {
        let g = PriceGranularity::medium();
        let params = TargetingParams {
            bidder: "appnexus",
            price: 1.55,
            bid_id: "bid1",
            imp_id: "imp1",
            size: Some((300, 250)),
            deal_id: Some("deal-123"),
            format: Some("banner"),
            cache_id: Some("cache-abc"),
            vast_cache_id: None,
            cache_host: None,
            cache_path: None,
            env: None,
            is_winning_bid: true,
            category_duration: None,
            always_include_deals: false,
            include_bidder_keys: true,
            include_winners: true,
        };
        let kv = make_targeting(&params, &g);

        // Bidder-suffixed keys
        assert_eq!(kv["hb_bidder_appnexus"], "appnexus");
        assert_eq!(kv["hb_pb_appnexus"], "1.50");
        assert_eq!(kv["hb_size_appnexus"], "300x250");
        assert_eq!(kv["hb_deal_appnexus"], "deal-123");

        // Winning (unsuffixed) keys
        assert_eq!(kv["hb_bidder"], "appnexus");
        assert_eq!(kv["hb_pb"], "1.50");
        assert_eq!(kv["hb_size"], "300x250");
        assert_eq!(kv["hb_deal"], "deal-123");
    }

    #[test]
    fn test_make_targeting_non_winning() {
        let g = PriceGranularity::medium();
        let params = TargetingParams {
            bidder: "rubicon",
            price: 2.0,
            bid_id: "bid2",
            imp_id: "imp1",
            size: None,
            deal_id: None,
            format: None,
            cache_id: None,
            vast_cache_id: None,
            cache_host: None,
            cache_path: None,
            env: None,
            is_winning_bid: false,
            category_duration: None,
            always_include_deals: false,
            include_bidder_keys: true,
            include_winners: true,
        };
        let kv = make_targeting(&params, &g);

        assert_eq!(kv["hb_bidder_rubicon"], "rubicon");
        assert_eq!(kv["hb_pb_rubicon"], "2.00");
        // No unsuffixed keys
        assert!(!kv.contains_key("hb_bidder"));
        assert!(!kv.contains_key("hb_pb"));
    }

    #[test]
    fn test_adserver_targeting_static() {
        let rules = vec![AdServerTargetingRule {
            key: "custom_key".to_string(),
            source: "static".to_string(),
            value: "hello".to_string(),
        }];
        let req = openrtb::BidRequest::default();
        let result = apply_adserver_targeting(&rules, &req, None);
        assert_eq!(result["custom_key"], "hello");
    }
}
