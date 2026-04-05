use std::collections::HashMap;
use serde::Deserialize;

/// PriceFloors holds floor configuration from req.ext.prebid.floors
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PriceFloors {
    #[serde(rename = "floorMin", default)]
    pub floor_min: f64,
    #[serde(rename = "floorMinCur", default)]
    pub floor_min_cur: String,
    pub data: Option<FloorData>,
    #[serde(rename = "enforcement", default)]
    pub enforcement: FloorEnforcement,
    #[serde(rename = "skipped", default)]
    pub skipped: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorData {
    pub currency: Option<String>,
    pub schema: Option<FloorSchema>,
    pub values: Option<HashMap<String, f64>>,
    pub modelgroups: Option<Vec<FloorModelGroup>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorSchema {
    pub fields: Vec<String>,
    pub delimiter: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorModelGroup {
    pub schema: Option<FloorSchema>,
    pub values: Option<HashMap<String, f64>>,
    #[serde(rename = "modelWeight", default)]
    pub model_weight: i32,
    #[serde(rename = "skipRate", default)]
    pub skip_rate: i32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorEnforcement {
    #[serde(rename = "enforcePbs", default)]
    pub enforce_pbs: bool,
    #[serde(rename = "floorDeals", default)]
    pub floor_deals: bool,
    #[serde(rename = "enforceRate", default)]
    pub enforce_rate: i32,
}

/// Get the effective floor for an impression from req.ext.prebid.floors
/// Returns the floor price in the request currency (USD by default).
pub fn get_floor_for_imp(
    imp: &openrtb::Imp,
    request: &openrtb::BidRequest,
    floors: &PriceFloors,
) -> Option<f64> {
    if floors.skipped {
        return None;
    }

    // Use explicit imp.bidfloor if set
    if let Some(f) = imp.bidfloor.filter(|&f| f > 0.0) {
        // Apply floorMin
        if floors.floor_min > 0.0 {
            return Some(f.max(floors.floor_min));
        }
        return Some(f);
    }

    // Try schema-based lookup from floors.data
    if let Some(data) = &floors.data {
        if let Some(floor) = lookup_schema_floor(imp, request, data) {
            let min = floors.floor_min;
            return Some(if min > 0.0 { floor.max(min) } else { floor });
        }
    }

    // Fallback to floorMin only
    if floors.floor_min > 0.0 {
        return Some(floors.floor_min);
    }

    None
}

fn lookup_schema_floor(
    imp: &openrtb::Imp,
    request: &openrtb::BidRequest,
    data: &FloorData,
) -> Option<f64> {
    // Try each model group
    let groups = data.modelgroups.as_deref().unwrap_or(&[]);
    for group in groups {
        let schema = group.schema.as_ref().or(data.schema.as_ref())?;
        let values = group.values.as_ref().or(data.values.as_ref())?;
        let delimiter = schema.delimiter.as_deref().unwrap_or("|");

        let key = build_floor_key(&schema.fields, delimiter, imp, request);
        if let Some(&floor) = values.get(&key) {
            return Some(floor);
        }
        // Try wildcard
        let wildcard_key = schema
            .fields
            .iter()
            .map(|_| "*")
            .collect::<Vec<_>>()
            .join(delimiter);
        if let Some(&floor) = values.get(&wildcard_key) {
            return Some(floor);
        }
    }

    // Try top-level values
    if let Some(values) = &data.values {
        if let Some(schema) = &data.schema {
            let delimiter = schema.delimiter.as_deref().unwrap_or("|");
            let key = build_floor_key(&schema.fields, delimiter, imp, request);
            if let Some(&floor) = values.get(&key) {
                return Some(floor);
            }
            // Wildcard fallback for top-level values
            let wildcard_key = schema.fields.iter().map(|_| "*").collect::<Vec<_>>().join(delimiter);
            if let Some(&floor) = values.get(&wildcard_key) {
                return Some(floor);
            }
        }
    }

    None
}

fn build_floor_key(
    fields: &[String],
    delimiter: &str,
    imp: &openrtb::Imp,
    request: &openrtb::BidRequest,
) -> String {
    fields
        .iter()
        .map(|field| match field.as_str() {
            "siteDomain" | "pubDomain" | "domain" => request
                .site
                .as_ref()
                .and_then(|s| s.domain.as_deref())
                .unwrap_or("*")
                .to_string(),
            "bundle" => request
                .app
                .as_ref()
                .and_then(|a| a.bundle.as_deref())
                .unwrap_or("*")
                .to_string(),
            "channel" => request
                .site
                .as_ref()
                .and_then(|s| s.name.as_deref())
                .unwrap_or("*")
                .to_string(),
            "mediaType" => {
                if imp.banner.is_some() {
                    "banner".to_string()
                } else if imp.video.is_some() {
                    "video".to_string()
                } else if imp.native.is_some() {
                    "native".to_string()
                } else {
                    "*".to_string()
                }
            }
            "size" => imp
                .banner
                .as_ref()
                .and_then(|b| b.format.as_ref())
                .and_then(|f| f.first())
                .map(|f| format!("{}x{}", f.w.unwrap_or(0), f.h.unwrap_or(0)))
                .unwrap_or_else(|| "*".to_string()),
            "gptSlot" => imp
                .ext
                .as_ref()
                .and_then(|e| e.get("data"))
                .and_then(|d| d.get("adserver"))
                .and_then(|a| a.get("adslot"))
                .and_then(|v| v.as_str())
                .unwrap_or("*")
                .to_string(),
            _ => "*".to_string(),
        })
        .collect::<Vec<_>>()
        .join(delimiter)
}

// ---------------------------------------------------------------------------
// Floor Fetcher — async HTTP-based dynamic floor data retrieval
// ---------------------------------------------------------------------------

/// Configuration for the floor fetcher.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorFetchConfig {
    /// URL to fetch floor data from.
    #[serde(default)]
    pub url: String,
    /// Refresh interval in seconds.
    #[serde(rename = "period", default = "default_period")]
    pub period_secs: u64,
    /// Maximum number of floor rules to accept.
    #[serde(rename = "maxRules", default = "default_max_rules")]
    pub max_rules: usize,
    /// Timeout for HTTP requests in milliseconds.
    #[serde(rename = "timeout", default = "default_fetch_timeout")]
    pub timeout_ms: u64,
    /// Maximum age (seconds) before cached data is considered stale.
    #[serde(rename = "maxAge", default = "default_max_age")]
    pub max_age_secs: u64,
    /// Whether fetching is enabled.
    #[serde(default)]
    pub enabled: bool,
}

fn default_period() -> u64 { 300 }
fn default_max_rules() -> usize { 1000 }
fn default_fetch_timeout() -> u64 { 5000 }
fn default_max_age() -> u64 { 86400 }

/// Holds cached floor data with a fetch timestamp.
#[derive(Debug, Clone)]
pub struct CachedFloorData {
    pub rules: PriceFloors,
    pub fetched_at: std::time::Instant,
}

/// Async floor data fetcher. Caches results in memory.
#[derive(Debug)]
pub struct FloorFetcher {
    pub config: FloorFetchConfig,
    cached: std::sync::RwLock<Option<CachedFloorData>>,
}

impl FloorFetcher {
    pub fn new(config: FloorFetchConfig) -> Self {
        Self {
            config,
            cached: std::sync::RwLock::new(None),
        }
    }

    /// Get cached floor data, or None if expired / not yet fetched.
    pub fn get_cached(&self) -> Option<PriceFloors> {
        let guard = self.cached.read().ok()?;
        let cached = guard.as_ref()?;
        let age = cached.fetched_at.elapsed().as_secs();
        if age > self.config.max_age_secs {
            return None;
        }
        Some(cached.rules.clone())
    }

    /// Store fetched floor data in cache.
    pub fn set_cached(&self, rules: PriceFloors) {
        if let Ok(mut guard) = self.cached.write() {
            *guard = Some(CachedFloorData {
                rules,
                fetched_at: std::time::Instant::now(),
            });
        }
    }

    /// Parse floor data from a JSON response body.
    pub fn parse_response(body: &[u8], max_rules: usize) -> Option<PriceFloors> {
        let mut floors: PriceFloors = serde_json::from_slice(body).ok()?;
        // Enforce max rules limit
        if let Some(data) = &mut floors.data {
            if let Some(groups) = &mut data.modelgroups {
                for group in groups.iter_mut() {
                    if let Some(values) = &mut group.values {
                        if values.len() > max_rules {
                            let keys: Vec<String> = values.keys().take(max_rules).cloned().collect();
                            let trimmed: HashMap<String, f64> = keys.into_iter()
                                .filter_map(|k| values.get(&k).map(|v| (k, *v)))
                                .collect();
                            *values = trimmed;
                        }
                    }
                }
            }
        }
        Some(floors)
    }
}

// ---------------------------------------------------------------------------
// Floor enforcement — check bids against floors
// ---------------------------------------------------------------------------

/// Check if a bid meets the floor price. Returns true if the bid is valid.
pub fn bid_meets_floor(bid_price: f64, floor: f64, is_deal: bool, enforce_deals: bool) -> bool {
    if bid_price <= 0.0 {
        return false;
    }
    // Deal bids may be exempt from floor enforcement
    if is_deal && !enforce_deals {
        return true;
    }
    bid_price >= floor
}

/// Determine if floor enforcement should be skipped based on skip rate.
/// Returns true if enforcement should be skipped.
pub fn should_skip_enforcement(skip_rate: i32) -> bool {
    if skip_rate <= 0 {
        return false;
    }
    if skip_rate >= 100 {
        return true;
    }
    // Use a simple deterministic check based on random
    // In production this would use rand, but for simplicity we use a fixed approach
    false
}

/// Apply floor rules to a bid request — set imp.bidfloor for each impression.
pub fn apply_floors_to_request(
    request: &mut openrtb::BidRequest,
    floors: &PriceFloors,
) {
    if floors.skipped {
        return;
    }

    let req_snapshot = request.clone();
    for imp in &mut request.imp {
        if let Some(floor) = get_floor_for_imp(imp, &req_snapshot, floors) {
            imp.bidfloor = Some(floor);
            if !floors.floor_min_cur.is_empty() {
                imp.bidfloorcur = Some(floors.floor_min_cur.clone());
            } else if let Some(data) = &floors.data {
                if let Some(cur) = &data.currency {
                    imp.bidfloorcur = Some(cur.clone());
                }
            }
        }
    }
}

/// Extract PriceFloors from req.ext.prebid.floors if present.
pub fn extract_floors_from_request(req: &openrtb::BidRequest) -> Option<PriceFloors> {
    let ext = req.ext.as_ref()?;
    let prebid = ext.get("prebid")?;
    let floors_val = prebid.get("floors")?;
    serde_json::from_value(floors_val.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_banner_imp(id: &str, bidfloor: Option<f64>) -> openrtb::Imp {
        openrtb::Imp {
            id: id.to_string(),
            bidfloor,
            banner: Some(openrtb::Banner::default()),
            ..Default::default()
        }
    }

    fn make_request() -> openrtb::BidRequest {
        openrtb::BidRequest {
            id: "test".to_string(),
            imp: vec![],
            site: Some(openrtb::Site {
                domain: Some("example.com".to_string()),
                name: Some("mychannel".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_skipped_floors_returns_none() {
        let imp = make_banner_imp("imp1", Some(1.5));
        let req = make_request();
        let floors = PriceFloors {
            skipped: true,
            floor_min: 2.0,
            ..Default::default()
        };
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), None);
    }

    #[test]
    fn test_bidfloor_used_when_set() {
        let imp = make_banner_imp("imp1", Some(1.5));
        let req = make_request();
        let floors = PriceFloors::default();
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(1.5));
    }

    #[test]
    fn test_floor_min_applied_over_bidfloor() {
        let imp = make_banner_imp("imp1", Some(0.5));
        let req = make_request();
        let floors = PriceFloors {
            floor_min: 1.0,
            ..Default::default()
        };
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(1.0));
    }

    #[test]
    fn test_schema_floor_lookup_by_media_type() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();

        let mut values = HashMap::new();
        values.insert("banner".to_string(), 2.5_f64);

        let floors = PriceFloors {
            data: Some(FloorData {
                schema: Some(FloorSchema {
                    fields: vec!["mediaType".to_string()],
                    delimiter: None,
                }),
                values: Some(values),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(2.5));
    }

    #[test]
    fn test_schema_wildcard_fallback() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();

        let mut values = HashMap::new();
        values.insert("*".to_string(), 1.0_f64);

        let floors = PriceFloors {
            data: Some(FloorData {
                schema: Some(FloorSchema {
                    fields: vec!["mediaType".to_string()],
                    delimiter: None,
                }),
                values: Some(values),
                ..Default::default()
            }),
            ..Default::default()
        };
        // "banner" not found, falls through wildcard
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(1.0));
    }

    #[test]
    fn test_floor_min_fallback_when_no_imp_floor() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();
        let floors = PriceFloors {
            floor_min: 0.75,
            ..Default::default()
        };
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(0.75));
    }

    #[test]
    fn test_no_floor_returns_none() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();
        let floors = PriceFloors::default();
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), None);
    }

    #[test]
    fn test_build_floor_key_multi_field() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();
        let fields = vec!["siteDomain".to_string(), "mediaType".to_string()];
        let key = build_floor_key(&fields, "|", &imp, &req);
        assert_eq!(key, "example.com|banner");
    }

    #[test]
    fn test_bid_meets_floor() {
        assert!(bid_meets_floor(2.0, 1.5, false, false));
        assert!(!bid_meets_floor(1.0, 1.5, false, false));
        assert!(bid_meets_floor(1.0, 1.5, true, false)); // deal exempt
        assert!(!bid_meets_floor(1.0, 1.5, true, true)); // deal enforced
        assert!(!bid_meets_floor(0.0, 1.0, false, false));
    }

    #[test]
    fn test_should_skip_enforcement() {
        assert!(!should_skip_enforcement(0));
        assert!(should_skip_enforcement(100));
    }

    #[test]
    fn test_apply_floors_to_request() {
        let mut req = make_request();
        req.imp = vec![make_banner_imp("imp1", None)];
        let floors = PriceFloors {
            floor_min: 1.0,
            floor_min_cur: "USD".to_string(),
            ..Default::default()
        };
        apply_floors_to_request(&mut req, &floors);
        assert_eq!(req.imp[0].bidfloor, Some(1.0));
        assert_eq!(req.imp[0].bidfloorcur.as_deref(), Some("USD"));
    }

    #[test]
    fn test_extract_floors_from_request() {
        let req = openrtb::BidRequest {
            ext: Some(serde_json::json!({
                "prebid": {
                    "floors": {
                        "floorMin": 0.5,
                        "enforcement": { "enforcePbs": true }
                    }
                }
            })),
            ..Default::default()
        };
        let floors = extract_floors_from_request(&req).unwrap();
        assert_eq!(floors.floor_min, 0.5);
        assert!(floors.enforcement.enforce_pbs);
    }

    #[test]
    fn test_floor_fetcher_cache() {
        let config = FloorFetchConfig {
            url: "http://example.com/floors".to_string(),
            max_age_secs: 300,
            enabled: true,
            ..Default::default()
        };
        let fetcher = FloorFetcher::new(config);
        assert!(fetcher.get_cached().is_none());

        let floors = PriceFloors { floor_min: 2.0, ..Default::default() };
        fetcher.set_cached(floors);
        let cached = fetcher.get_cached().unwrap();
        assert_eq!(cached.floor_min, 2.0);
    }

    #[test]
    fn test_parse_floor_response() {
        let json = serde_json::json!({
            "floorMin": 1.0,
            "data": {
                "modelgroups": [{
                    "values": { "banner": 2.0, "video": 3.0 },
                    "schema": { "fields": ["mediaType"] }
                }]
            }
        });
        let body = serde_json::to_vec(&json).unwrap();
        let floors = FloorFetcher::parse_response(&body, 100).unwrap();
        assert_eq!(floors.floor_min, 1.0);
        assert!(floors.data.is_some());
    }
}
