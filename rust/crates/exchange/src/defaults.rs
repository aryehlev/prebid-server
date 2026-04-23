//! Set default values for OpenRTB requests.
//! Mirrors Go `ortb/default.go`.

use openrtb::BidRequest;
use openrtb_ext::{ExtRequestPrebid, PriceGranularity, GranularityRange};

pub const DEFAULT_PRICE_GRANULARITY_PRECISION: i32 = 2;
pub const DEFAULT_TARGETING_INCLUDE_WINNERS: bool = true;
pub const DEFAULT_TARGETING_INCLUDE_BIDDER_KEYS: bool = true;
pub const DEFAULT_SECURE: i32 = 1;

/// Set defaults on a BidRequest. Returns true if any modifications were made.
pub fn set_defaults(req: &mut BidRequest, default_tmax: i64) -> bool {
    let mut modified = false;

    // Set default tmax
    if req.tmax.is_none() || req.tmax == Some(0) {
        req.tmax = Some(default_tmax);
        modified = true;
    }

    // Set secure=1 on all impressions that don't have it set
    for imp in &mut req.imp {
        if imp.secure.is_none() {
            imp.secure = Some(DEFAULT_SECURE);
            modified = true;
        }
    }

    // Set targeting defaults in ext.prebid.targeting
    if let Some(ref mut ext) = req.ext {
        if let Ok(mut prebid) = serde_json::from_value::<ExtRequestPrebid>(
            ext.get("prebid").cloned().unwrap_or(serde_json::Value::Null)
        ) {
            if let Some(ref mut targeting) = prebid.targeting {
                let targeting_modified = set_defaults_targeting(targeting);
                if targeting_modified {
                    if let Ok(prebid_val) = serde_json::to_value(&prebid) {
                        ext.as_object_mut().map(|m| m.insert("prebid".to_string(), prebid_val));
                        modified = true;
                    }
                }
            }
        }
    }

    modified
}

/// Set defaults for targeting configuration. Returns true if modified.
fn set_defaults_targeting(targeting: &mut openrtb_ext::ExtRequestTargeting) -> bool {
    let mut modified = false;

    // Set default price granularity
    if let Some(ref mut pg) = targeting.pricegranularity {
        if set_defaults_price_granularity(pg) {
            modified = true;
        }
    } else {
        targeting.pricegranularity = Some(PriceGranularity::new_default());
        modified = true;
    }

    // Set default for media type price granularity
    if let Some(ref mut mtpg) = targeting.mediatypepricegranularity {
        if let Some(ref mut video_pg) = mtpg.video {
            if set_defaults_price_granularity(video_pg) {
                modified = true;
            }
        }
        if let Some(ref mut banner_pg) = mtpg.banner {
            if set_defaults_price_granularity(banner_pg) {
                modified = true;
            }
        }
        if let Some(ref mut native_pg) = mtpg.native_type {
            if set_defaults_price_granularity(native_pg) {
                modified = true;
            }
        }
    }

    // Set default includewinners
    if targeting.includewinners.is_none() {
        targeting.includewinners = Some(DEFAULT_TARGETING_INCLUDE_WINNERS);
        modified = true;
    }

    // Set default includebidderkeys
    if targeting.includebidderkeys.is_none() {
        targeting.includebidderkeys = Some(DEFAULT_TARGETING_INCLUDE_BIDDER_KEYS);
        modified = true;
    }

    modified
}

/// Set defaults for price granularity. Returns true if modified.
fn set_defaults_price_granularity(pg: &mut PriceGranularity) -> bool {
    let mut modified = false;

    // Default precision to 2
    if pg.precision.is_none() {
        pg.precision = Some(DEFAULT_PRICE_GRANULARITY_PRECISION);
        modified = true;
    }

    // Set default ranges if empty
    if pg.ranges.as_ref().map(|r| r.is_empty()).unwrap_or(true) {
        pg.ranges = Some(vec![
            GranularityRange { min: 0.0, max: 20.0, increment: 0.1 },
        ]);
        modified = true;
    }

    // Ensure range min values are continuous
    if let Some(ref mut ranges) = pg.ranges {
        let mut prev_max = 0.0;
        for range in ranges.iter_mut() {
            if range.min != prev_max {
                range.min = prev_max;
                modified = true;
            }
            prev_max = range.max;
        }
    }

    modified
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_defaults_tmax() {
        let mut req = BidRequest::default();
        req.imp.push(openrtb::Imp { id: "1".to_string(), ..Default::default() });
        assert!(set_defaults(&mut req, 500));
        assert_eq!(req.tmax, Some(500));
    }

    #[test]
    fn test_set_defaults_secure() {
        let mut req = BidRequest::default();
        req.imp.push(openrtb::Imp { id: "1".to_string(), ..Default::default() });
        set_defaults(&mut req, 500);
        assert_eq!(req.imp[0].secure, Some(1));
    }

    #[test]
    fn test_set_defaults_secure_already_set() {
        let mut req = BidRequest::default();
        req.imp.push(openrtb::Imp { id: "1".to_string(), secure: Some(0), ..Default::default() });
        set_defaults(&mut req, 500);
        assert_eq!(req.imp[0].secure, Some(0));
    }

    #[test]
    fn test_set_defaults_price_granularity() {
        let mut pg = PriceGranularity {
            precision: None,
            ranges: Some(vec![
                GranularityRange { min: 0.0, max: 5.0, increment: 0.05 },
                GranularityRange { min: 0.0, max: 20.0, increment: 0.5 },
            ]),
        };
        assert!(set_defaults_price_granularity(&mut pg));
        assert_eq!(pg.precision, Some(2));
        // Second range should have min adjusted to 5.0
        assert_eq!(pg.ranges.as_ref().unwrap()[1].min, 5.0);
    }
}
