//! VAST XML generation and manipulation.
//! Mirrors Go `exchange/auction.go` VAST functions and `injector/injector.go`.
//!
//! Provides VAST XML generation from bid AdM/NURL and event URL injection.

/// Generate VAST XML for a bid.
/// If AdM is defined it takes precedence, otherwise wraps the NURL in a redirect tag.
/// Mirrors Go `makeVAST`.
pub fn make_vast(adm: &str, nurl: &str) -> String {
    if !adm.is_empty() {
        return adm.to_string();
    }
    format!(
        r#"<VAST version="3.0"><Ad><Wrapper><AdSystem>prebid.org wrapper</AdSystem><VASTAdTagURI><![CDATA[{}]]></VASTAdTagURI><Impression></Impression><Creatives></Creatives></Wrapper></Ad></VAST>"#,
        nurl
    )
}

/// Calculate the cache TTL from imp, bid, and default TTLs with a buffer.
/// Mirrors Go `cacheTTL`.
pub fn cache_ttl(imp_ttl: i64, bid_ttl: i64, def_ttl: i64, buffer: i64) -> i64 {
    if imp_ttl <= 0 && bid_ttl <= 0 {
        return add_buffer(def_ttl, buffer);
    }
    if imp_ttl <= 0 {
        return add_buffer(bid_ttl, buffer);
    }
    if bid_ttl <= 0 {
        return add_buffer(imp_ttl, buffer);
    }
    if imp_ttl < bid_ttl {
        add_buffer(imp_ttl, buffer)
    } else {
        add_buffer(bid_ttl, buffer)
    }
}

fn add_buffer(base: i64, buffer: i64) -> i64 {
    if base <= 0 {
        return 0;
    }
    base + buffer
}

/// Check if the debug override header matches the configured token.
/// Mirrors Go `IsDebugOverrideEnabled`.
pub fn is_debug_override_enabled(debug_header: &str, config_override_token: &str) -> bool {
    !config_override_token.is_empty() && debug_header == config_override_token
}

/// Debug log structure for tracking request/response data.
/// Mirrors Go `DebugLog`.
#[derive(Debug, Clone, Default)]
pub struct DebugLog {
    pub enabled: bool,
    pub data: DebugData,
    pub ttl: i64,
    pub cache_key: String,
    pub cache_string: String,
    pub debug_override: bool,
    pub debug_enabled_or_overridden: bool,
}

/// Debug data containing request, headers, and response info.
#[derive(Debug, Clone, Default)]
pub struct DebugData {
    pub request: String,
    pub headers: String,
    pub response: String,
}

impl DebugLog {
    /// Build the cache string from debug data in XML format.
    /// Mirrors Go `DebugLog.BuildCacheString`.
    pub fn build_cache_string(&mut self) {
        self.cache_string = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><Log><Request>{}</Request><Headers>{}</Headers><Response>{}</Response></Log>"#,
            self.data.request, self.data.headers, self.data.response
        );
    }
}

/// Bid validation — check that a bid has required fields and valid values.
/// Mirrors Go `validateBid`.
pub fn validate_bid(
    bid_id: &str,
    imp_id: &str,
    price: f64,
    deal_id: &str,
    cr_id: &str,
    debug: bool,
) -> Result<(), Option<String>> {
    if bid_id.is_empty() {
        return Err(Some("Bid missing required field 'id'".to_string()));
    }
    if imp_id.is_empty() {
        return Err(Some(format!(
            "Bid \"{}\" missing required field 'impid'",
            bid_id
        )));
    }
    if price < 0.0 {
        if debug {
            return Err(Some(format!(
                "Bid \"{}\" does not contain a positive (or zero if there is a deal) 'price'",
                bid_id
            )));
        }
        return Err(None);
    }
    if price == 0.0 && deal_id.is_empty() {
        if debug {
            return Err(Some(format!(
                "Bid \"{}\" does not contain positive 'price' which is required since there is no deal set for this bid",
                bid_id
            )));
        }
        return Err(None);
    }
    if cr_id.is_empty() {
        return Err(Some(format!(
            "Bid \"{}\" missing creative ID",
            bid_id
        )));
    }
    Ok(())
}

/// Validate that the bid currency is an allowed ISO currency.
/// Mirrors Go `validateCurrency`.
pub fn validate_currency(
    request_allowed_currencies: &[String],
    bid_currency: &str,
) -> Result<(), String> {
    let bid_cur = if bid_currency.is_empty() {
        "USD"
    } else {
        bid_currency
    };

    let allowed = if request_allowed_currencies.is_empty() {
        vec!["USD".to_string()]
    } else {
        request_allowed_currencies.to_vec()
    };

    let bid_cur_upper = bid_cur.to_uppercase();

    // Simple ISO 4217 currency code validation (3 uppercase letters)
    if bid_cur_upper.len() != 3 || !bid_cur_upper.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(format!("Invalid currency code: '{}'", bid_cur));
    }

    for allowed_cur in &allowed {
        if allowed_cur.to_uppercase() == bid_cur_upper {
            return Ok(());
        }
    }

    let allowed_str: Vec<String> = allowed.iter().map(|s| format!("'{}'", s)).collect();
    Err(format!(
        "Bid currency is not allowed. Was '{}', wants: [{}]",
        bid_cur_upper,
        allowed_str.join(", ")
    ))
}

/// Determine if a new bid wins against the current winning bid.
/// Mirrors Go `isNewWinningBid`.
pub fn is_new_winning_bid(
    bid_price: f64,
    bid_has_deal: bool,
    winning_price: f64,
    winning_has_deal: bool,
    prefer_deals: bool,
) -> bool {
    if prefer_deals {
        if winning_has_deal && !bid_has_deal {
            return false;
        }
        if !winning_has_deal && bid_has_deal {
            return true;
        }
    }
    bid_price > winning_price
}

/// NonBidReason codes.
/// Reference: IAB OpenRTB seat-non-bid extension.
/// Mirrors Go `NonBidReason` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i64)]
pub enum NonBidReason {
    ErrorGeneral = 100,
    ErrorTimeout = 101,
    ErrorBidderUnreachable = 103,
    ResponseRejectedGeneral = 300,
    ResponseRejectedBelowFloor = 301,
    ResponseRejectedCategoryMappingInvalid = 303,
    ResponseRejectedBelowDealFloor = 304,
    ResponseRejectedCreativeSizeNotAllowed = 351,
    ResponseRejectedCreativeNotSecure = 352,
}

impl NonBidReason {
    pub fn code(self) -> i64 {
        self as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_vast_with_adm() {
        let result = make_vast("<VAST>ad markup</VAST>", "http://nurl.com");
        assert_eq!(result, "<VAST>ad markup</VAST>");
    }

    #[test]
    fn test_make_vast_with_nurl() {
        let result = make_vast("", "http://nurl.com");
        assert!(result.contains("prebid.org wrapper"));
        assert!(result.contains("http://nurl.com"));
    }

    #[test]
    fn test_cache_ttl_both_zero() {
        assert_eq!(cache_ttl(0, 0, 60, 10), 70);
    }

    #[test]
    fn test_cache_ttl_imp_only() {
        assert_eq!(cache_ttl(30, 0, 60, 10), 40);
    }

    #[test]
    fn test_cache_ttl_bid_only() {
        assert_eq!(cache_ttl(0, 45, 60, 10), 55);
    }

    #[test]
    fn test_cache_ttl_both_set_imp_smaller() {
        assert_eq!(cache_ttl(20, 45, 60, 10), 30);
    }

    #[test]
    fn test_cache_ttl_both_set_bid_smaller() {
        assert_eq!(cache_ttl(50, 30, 60, 10), 40);
    }

    #[test]
    fn test_cache_ttl_negative() {
        assert_eq!(cache_ttl(-1, -1, 0, 10), 0);
    }

    #[test]
    fn test_debug_override_enabled() {
        assert!(is_debug_override_enabled("token123", "token123"));
        assert!(!is_debug_override_enabled("wrong", "token123"));
        assert!(!is_debug_override_enabled("token123", ""));
    }

    #[test]
    fn test_validate_bid_valid() {
        assert!(validate_bid("bid1", "imp1", 1.5, "", "cr1", false).is_ok());
    }

    #[test]
    fn test_validate_bid_missing_id() {
        let err = validate_bid("", "imp1", 1.5, "", "cr1", false).unwrap_err();
        assert!(err.unwrap().contains("missing required field 'id'"));
    }

    #[test]
    fn test_validate_bid_missing_impid() {
        let err = validate_bid("bid1", "", 1.5, "", "cr1", false).unwrap_err();
        assert!(err.unwrap().contains("impid"));
    }

    #[test]
    fn test_validate_bid_negative_price() {
        assert!(validate_bid("bid1", "imp1", -1.0, "", "cr1", true)
            .unwrap_err()
            .is_some());
        assert!(validate_bid("bid1", "imp1", -1.0, "", "cr1", false)
            .unwrap_err()
            .is_none());
    }

    #[test]
    fn test_validate_bid_zero_price_no_deal() {
        assert!(validate_bid("bid1", "imp1", 0.0, "", "cr1", true)
            .unwrap_err()
            .is_some());
    }

    #[test]
    fn test_validate_bid_zero_price_with_deal() {
        assert!(validate_bid("bid1", "imp1", 0.0, "deal1", "cr1", false).is_ok());
    }

    #[test]
    fn test_validate_bid_missing_crid() {
        let err = validate_bid("bid1", "imp1", 1.5, "", "", false).unwrap_err();
        assert!(err.unwrap().contains("creative ID"));
    }

    #[test]
    fn test_validate_currency_default() {
        assert!(validate_currency(&[], "").is_ok());
        assert!(validate_currency(&[], "USD").is_ok());
    }

    #[test]
    fn test_validate_currency_allowed() {
        let allowed = vec!["USD".to_string(), "EUR".to_string()];
        assert!(validate_currency(&allowed, "EUR").is_ok());
    }

    #[test]
    fn test_validate_currency_not_allowed() {
        let allowed = vec!["USD".to_string()];
        let result = validate_currency(&allowed, "EUR");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not allowed"));
    }

    #[test]
    fn test_validate_currency_invalid() {
        let result = validate_currency(&[], "INVALID");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_new_winning_bid_higher_price() {
        assert!(is_new_winning_bid(2.0, false, 1.0, false, false));
        assert!(!is_new_winning_bid(1.0, false, 2.0, false, false));
    }

    #[test]
    fn test_is_new_winning_bid_prefer_deals() {
        // Deal bid wins over non-deal bid
        assert!(is_new_winning_bid(1.0, true, 2.0, false, true));
        // Non-deal bid loses to deal bid
        assert!(!is_new_winning_bid(2.0, false, 1.0, true, true));
        // Both have deals, higher price wins
        assert!(is_new_winning_bid(2.0, true, 1.0, true, true));
    }

    #[test]
    fn test_non_bid_reason_codes() {
        assert_eq!(NonBidReason::ErrorGeneral.code(), 100);
        assert_eq!(NonBidReason::ErrorTimeout.code(), 101);
        assert_eq!(NonBidReason::ResponseRejectedBelowFloor.code(), 301);
    }

    #[test]
    fn test_debug_log_build_cache_string() {
        let mut log = DebugLog {
            data: DebugData {
                request: "req".to_string(),
                headers: "hdrs".to_string(),
                response: "resp".to_string(),
            },
            ..Default::default()
        };
        log.build_cache_string();
        assert!(log.cache_string.contains("<Request>req</Request>"));
        assert!(log.cache_string.contains("<Headers>hdrs</Headers>"));
        assert!(log.cache_string.contains("<Response>resp</Response>"));
    }
}
