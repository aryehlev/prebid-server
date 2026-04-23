use std::collections::HashMap;

// ---------------------------------------------------------------------------
// NonBidReason
// ---------------------------------------------------------------------------

/// Reasons why a bid was not submitted, matching the Go `NonBidReason` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum NonBidReason {
    /// No reason specified.
    NoBidUnknown = 0,

    // --- Error range (100+) ---
    ErrorGeneral = 100,
    ErrorTimeout = 101,
    ErrorBidderUnreachable = 102,
    ErrorBadInput = 103,
    ErrorBadServerResponse = 104,
    ErrorFailedToMarshal = 105,

    // --- Response-rejected range (200+) ---
    ResponseRejectedGeneral = 200,
    ResponseRejectedBelowFloor = 201,
    ResponseRejectedCategoryMappingInvalid = 202,
    ResponseRejectedBelowDealFloor = 204,
    ResponseRejectedCreativeSizeNotAllowed = 205,
    ResponseRejectedNotQualifiedAdSlot = 206,
    ResponseRejectedInvalidCreative = 300,
    ResponseRejectedBidPriceCurrencyConversionError = 301,
    ResponseRejectedBidPriceBelowMinimum = 302,
    ResponseRejectedBidPriceNotPositive = 303,
}

impl NonBidReason {
    /// Returns the integer code for this reason.
    pub fn code(self) -> i32 {
        self as i32
    }
}

impl From<i32> for NonBidReason {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::NoBidUnknown,
            100 => Self::ErrorGeneral,
            101 => Self::ErrorTimeout,
            102 => Self::ErrorBidderUnreachable,
            103 => Self::ErrorBadInput,
            104 => Self::ErrorBadServerResponse,
            105 => Self::ErrorFailedToMarshal,
            200 => Self::ResponseRejectedGeneral,
            201 => Self::ResponseRejectedBelowFloor,
            202 => Self::ResponseRejectedCategoryMappingInvalid,
            204 => Self::ResponseRejectedBelowDealFloor,
            205 => Self::ResponseRejectedCreativeSizeNotAllowed,
            206 => Self::ResponseRejectedNotQualifiedAdSlot,
            300 => Self::ResponseRejectedInvalidCreative,
            301 => Self::ResponseRejectedBidPriceCurrencyConversionError,
            302 => Self::ResponseRejectedBidPriceBelowMinimum,
            303 => Self::ResponseRejectedBidPriceNotPositive,
            _ => Self::NoBidUnknown,
        }
    }
}

// ---------------------------------------------------------------------------
// Auction
// ---------------------------------------------------------------------------

/// Represents a single bid stored inside the auction.
#[derive(Debug, Clone)]
pub struct AuctionBid {
    pub bidder: String,
    pub imp_id: String,
    pub price: f64,
    pub deal_id: String,
}

/// Holds the results of the auction process.
#[derive(Debug, Clone, Default)]
pub struct Auction {
    /// Winning bid per imp ID.
    pub winning_bids: HashMap<String, AuctionBid>,
    /// All bids grouped by bidder name.
    pub all_bids_by_bidder: HashMap<String, Vec<AuctionBid>>,
    /// Rounded/bucketed price strings keyed by a bid identifier.
    pub rounded_prices: HashMap<String, String>,
    /// Cache IDs for banner/native bids.
    pub cache_ids: HashMap<String, String>,
    /// Cache IDs for VAST video bids.
    pub vast_cache_ids: HashMap<String, String>,
}

/// Determines whether a new bid should replace the current winning bid.
///
/// When `prefer_deals` is true a deal bid beats a non-deal bid regardless of
/// price. Otherwise the higher price wins; ties are broken in favour of the
/// incumbent (returns `false`).
pub fn is_new_winning_bid(
    bid_price: f64,
    bid_deal: &str,
    winning_price: f64,
    winning_deal: &str,
    prefer_deals: bool,
) -> bool {
    if prefer_deals {
        let new_has_deal = !bid_deal.is_empty();
        let old_has_deal = !winning_deal.is_empty();

        if new_has_deal && !old_has_deal {
            return true;
        }
        if !new_has_deal && old_has_deal {
            return false;
        }
    }

    bid_price > winning_price
}

// ---------------------------------------------------------------------------
// VAST helpers
// ---------------------------------------------------------------------------

/// Builds a VAST XML wrapper/document from `adm` and `nurl`.
///
/// - If `adm` is non-empty it is returned as-is (the ad markup already
///   contains the full VAST document).
/// - Otherwise a minimal VAST wrapper pointing at `nurl` is generated.
/// - If both are empty an empty string is returned.
pub fn make_vast(adm: &str, nurl: &str) -> String {
    if !adm.is_empty() {
        return adm.to_string();
    }
    if !nurl.is_empty() {
        return format!(
            r#"<VAST version="3.0"><Ad><Wrapper><AdSystem>prebid.org wrapper</AdSystem><VASTAdTagURI><![CDATA[{}]]></VASTAdTagURI><Impression></Impression><Creatives></Creatives></Wrapper></Ad></VAST>"#,
            nurl
        );
    }
    String::new()
}

// ---------------------------------------------------------------------------
// Cache TTL
// ---------------------------------------------------------------------------

/// Computes the effective cache TTL in seconds.
///
/// Priority: `bid_ttl` > `imp_ttl` > `def_ttl`.  The `buffer` is added to
/// give a small margin before expiry. A zero or negative result is clamped to
/// the `def_ttl + buffer`.
pub fn cache_ttl(imp_ttl: i64, bid_ttl: i64, def_ttl: i64, buffer: i64) -> i64 {
    let base = if bid_ttl > 0 {
        bid_ttl
    } else if imp_ttl > 0 {
        imp_ttl
    } else {
        def_ttl
    };
    let total = base + buffer;
    if total <= 0 {
        def_ttl + buffer
    } else {
        total
    }
}

// ---------------------------------------------------------------------------
// Debug helpers
// ---------------------------------------------------------------------------

/// Supplemental data collected during auction debugging.
#[derive(Debug, Clone, Default)]
pub struct DebugData {
    pub request_uri: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub status_code: u16,
}

/// Controls debug/test output for an auction.
#[derive(Debug, Clone, Default)]
pub struct DebugLog {
    pub enabled: bool,
    pub cache_type: String,
    pub data: DebugData,
    pub ttl: i64,
    pub regexp: String,
    pub debug_override_header: String,
}

/// Returns `true` when the debug override header matches the configured token
/// and the token is non-empty.
pub fn is_debug_override_enabled(debug_header: &str, config_override_token: &str) -> bool {
    !config_override_token.is_empty() && debug_header == config_override_token
}

// ---------------------------------------------------------------------------
// Bid validation  (ports exchange/bidder_validate_bids.go)
// ---------------------------------------------------------------------------

/// Validates a single bid's required fields.
///
/// Returns `Ok(())` when the bid is valid, or `Err(message)` describing the
/// first validation failure encountered.
pub fn validate_bid(
    id: &str,
    imp_id: &str,
    price: f64,
    deal_id: &str,
    cr_id: &str,
    _debug: bool,
) -> Result<(), String> {
    if id.is_empty() {
        return Err("bid missing required field: \"id\"".to_string());
    }
    if imp_id.is_empty() {
        return Err(format!(
            "bid \"{id}\" missing required field: \"impid\""
        ));
    }
    if price <= 0.0 && deal_id.is_empty() {
        return Err(format!(
            "bid \"{id}\" has non-positive price {price:.4} without a deal"
        ));
    }
    if cr_id.is_empty() {
        return Err(format!(
            "bid \"{id}\" missing required field: \"crid\""
        ));
    }
    Ok(())
}

/// Validates that the bid currency is acceptable given the request currencies.
///
/// - If `request_currencies` is empty any currency is accepted.
/// - If `bid_currency` is empty it is treated as USD by convention.
/// - Otherwise `bid_currency` must appear in `request_currencies`.
pub fn validate_currency(
    request_currencies: &[String],
    bid_currency: &str,
) -> Result<(), String> {
    if request_currencies.is_empty() {
        return Ok(());
    }

    let effective = if bid_currency.is_empty() {
        "USD"
    } else {
        bid_currency
    };

    if request_currencies.iter().any(|c| c == effective) {
        Ok(())
    } else {
        Err(format!(
            "bid currency \"{effective}\" is not in the list of request currencies: {:?}",
            request_currencies
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- NonBidReason --

    #[test]
    fn test_non_bid_reason_codes() {
        assert_eq!(NonBidReason::NoBidUnknown.code(), 0);
        assert_eq!(NonBidReason::ErrorGeneral.code(), 100);
        assert_eq!(NonBidReason::ErrorTimeout.code(), 101);
        assert_eq!(NonBidReason::ResponseRejectedBelowFloor.code(), 201);
        assert_eq!(NonBidReason::ResponseRejectedBidPriceNotPositive.code(), 303);
    }

    #[test]
    fn test_non_bid_reason_from_i32() {
        assert_eq!(NonBidReason::from(101), NonBidReason::ErrorTimeout);
        assert_eq!(NonBidReason::from(999), NonBidReason::NoBidUnknown);
    }

    // -- is_new_winning_bid --

    #[test]
    fn test_higher_price_wins() {
        assert!(is_new_winning_bid(2.0, "", 1.0, "", false));
    }

    #[test]
    fn test_lower_price_loses() {
        assert!(!is_new_winning_bid(1.0, "", 2.0, "", false));
    }

    #[test]
    fn test_equal_price_incumbent_wins() {
        assert!(!is_new_winning_bid(1.0, "", 1.0, "", false));
    }

    #[test]
    fn test_deal_beats_no_deal_when_preferred() {
        assert!(is_new_winning_bid(1.0, "deal1", 2.0, "", true));
    }

    #[test]
    fn test_no_deal_loses_to_deal_when_preferred() {
        assert!(!is_new_winning_bid(3.0, "", 1.0, "deal1", true));
    }

    #[test]
    fn test_both_deals_higher_price_wins() {
        assert!(is_new_winning_bid(3.0, "d1", 2.0, "d2", true));
    }

    #[test]
    fn test_no_prefer_deals_ignores_deals() {
        // Without prefer_deals, pure price comparison.
        assert!(!is_new_winning_bid(1.0, "deal1", 2.0, "", false));
    }

    // -- make_vast --

    #[test]
    fn test_make_vast_with_adm() {
        let adm = "<VAST>custom</VAST>";
        assert_eq!(make_vast(adm, "https://example.com/nurl"), adm);
    }

    #[test]
    fn test_make_vast_with_nurl() {
        let result = make_vast("", "https://example.com/vast");
        assert!(result.contains("VASTAdTagURI"));
        assert!(result.contains("https://example.com/vast"));
        assert!(result.starts_with("<VAST"));
    }

    #[test]
    fn test_make_vast_empty() {
        assert_eq!(make_vast("", ""), "");
    }

    // -- cache_ttl --

    #[test]
    fn test_cache_ttl_bid_priority() {
        assert_eq!(cache_ttl(100, 200, 300, 10), 210);
    }

    #[test]
    fn test_cache_ttl_imp_fallback() {
        assert_eq!(cache_ttl(100, 0, 300, 10), 110);
    }

    #[test]
    fn test_cache_ttl_default_fallback() {
        assert_eq!(cache_ttl(0, 0, 300, 10), 310);
    }

    #[test]
    fn test_cache_ttl_clamp_negative() {
        // bid_ttl = -50, buffer = 10 => base = -50 (> 0 check fails)
        // falls to def_ttl + buffer = 300 + 10 = 310
        assert_eq!(cache_ttl(0, -50, 300, 10), 310);
    }

    // -- debug --

    #[test]
    fn test_debug_override_enabled() {
        assert!(is_debug_override_enabled("secret", "secret"));
    }

    #[test]
    fn test_debug_override_wrong_token() {
        assert!(!is_debug_override_enabled("wrong", "secret"));
    }

    #[test]
    fn test_debug_override_empty_token() {
        assert!(!is_debug_override_enabled("anything", ""));
    }

    #[test]
    fn test_debug_override_both_empty() {
        assert!(!is_debug_override_enabled("", ""));
    }

    // -- validate_bid --

    #[test]
    fn test_validate_bid_valid() {
        assert!(validate_bid("b1", "imp1", 1.5, "", "cr1", false).is_ok());
    }

    #[test]
    fn test_validate_bid_missing_id() {
        let err = validate_bid("", "imp1", 1.0, "", "cr1", false).unwrap_err();
        assert!(err.contains("\"id\""));
    }

    #[test]
    fn test_validate_bid_missing_imp_id() {
        let err = validate_bid("b1", "", 1.0, "", "cr1", false).unwrap_err();
        assert!(err.contains("\"impid\""));
    }

    #[test]
    fn test_validate_bid_zero_price_no_deal() {
        let err = validate_bid("b1", "imp1", 0.0, "", "cr1", false).unwrap_err();
        assert!(err.contains("non-positive price"));
    }

    #[test]
    fn test_validate_bid_zero_price_with_deal() {
        // Zero price is acceptable when a deal is present.
        assert!(validate_bid("b1", "imp1", 0.0, "deal1", "cr1", false).is_ok());
    }

    #[test]
    fn test_validate_bid_missing_crid() {
        let err = validate_bid("b1", "imp1", 1.0, "", "", false).unwrap_err();
        assert!(err.contains("\"crid\""));
    }

    // -- validate_currency --

    #[test]
    fn test_validate_currency_empty_request() {
        assert!(validate_currency(&[], "EUR").is_ok());
    }

    #[test]
    fn test_validate_currency_match() {
        let currencies = vec!["USD".to_string(), "EUR".to_string()];
        assert!(validate_currency(&currencies, "EUR").is_ok());
    }

    #[test]
    fn test_validate_currency_no_match() {
        let currencies = vec!["USD".to_string()];
        let err = validate_currency(&currencies, "EUR").unwrap_err();
        assert!(err.contains("EUR"));
    }

    #[test]
    fn test_validate_currency_empty_bid_defaults_usd() {
        let currencies = vec!["USD".to_string()];
        assert!(validate_currency(&currencies, "").is_ok());
    }

    #[test]
    fn test_validate_currency_empty_bid_no_usd_in_request() {
        let currencies = vec!["EUR".to_string()];
        let err = validate_currency(&currencies, "").unwrap_err();
        assert!(err.contains("USD"));
    }
}
