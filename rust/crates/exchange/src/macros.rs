/// Macros module for URL macro replacement in prebid tracking URLs.
///
/// Handles all standard prebid macros that can appear in nurl, burl, adm and
/// other tracking URL fields of a bid.

/// All macro values that can be substituted into tracking URL templates.
pub struct MacroValues {
    pub auction_price: f64,
    pub auction_currency: String,
    pub auction_id: String,
    pub bidder_name: String,
    pub imp_id: String,
    pub timeout: u64,
    pub ad_markup: Option<String>,
}

/// Replace all known prebid macros in a URL template string.
///
/// Handles both plain and URL-encoded variants of `${AUCTION_PRICE}`.
pub fn replace_macros(template: &str, values: &MacroValues) -> String {
    let price_str_4 = format!("{:.4}", values.auction_price);
    let price_str_2 = format!("{:.2}", values.auction_price);

    template
        .replace("${AUCTION_PRICE}", &price_str_4)
        // URL-encoded variant: %24%7BAUCTION_PRICE%7D
        .replace("%24%7BAUCTION_PRICE%7D", &price_str_4)
        // Truncated 2-decimal variant
        .replace("${AUCTION_PRICE:2}", &price_str_2)
        .replace("${AUCTION_CURRENCY}", &values.auction_currency)
        .replace("${AUCTION_ID}", &values.auction_id)
        .replace("${AUCTION_BID_ID}", &values.imp_id)
        .replace("${AUCTION_IMP_ID}", &values.imp_id)
        .replace("${AUCTION_SEAT_ID}", &values.bidder_name)
        .replace(
            "${AUCTION_AD_ID}",
            values.ad_markup.as_deref().unwrap_or(""),
        )
}

/// Apply macro replacement to all tracking URL fields of a bid (nurl, burl, adm).
///
/// For `adm`, replacement is only performed when the field actually contains a
/// known macro token, because adm may contain large creative markup and the
/// macro substitution is not always desired.
pub fn apply_bid_macros(bid: &mut openrtb::Bid, values: &MacroValues) {
    if let Some(nurl) = &bid.nurl {
        bid.nurl = Some(replace_macros(nurl, values));
    }
    if let Some(burl) = &bid.burl {
        bid.burl = Some(replace_macros(burl, values));
    }
    if let Some(adm) = &bid.adm {
        if adm.contains("${AUCTION_PRICE}") || adm.contains("%24%7BAUCTION_PRICE%7D") {
            bid.adm = Some(replace_macros(adm, values));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_values() -> MacroValues {
        MacroValues {
            auction_price: 1.5,
            auction_currency: "USD".to_string(),
            auction_id: "auction-123".to_string(),
            bidder_name: "appnexus".to_string(),
            imp_id: "imp-456".to_string(),
            timeout: 1000,
            ad_markup: None,
        }
    }

    #[test]
    fn replaces_auction_price() {
        let values = sample_values();
        let result = replace_macros("https://example.com/win?price=${AUCTION_PRICE}", &values);
        assert_eq!(result, "https://example.com/win?price=1.5000");
    }

    #[test]
    fn replaces_url_encoded_auction_price() {
        let values = sample_values();
        let result =
            replace_macros("https://example.com/win?price=%24%7BAUCTION_PRICE%7D", &values);
        assert_eq!(result, "https://example.com/win?price=1.5000");
    }

    #[test]
    fn replaces_auction_price_2_decimals() {
        let values = sample_values();
        let result = replace_macros("https://example.com/win?price=${AUCTION_PRICE:2}", &values);
        assert_eq!(result, "https://example.com/win?price=1.50");
    }

    #[test]
    fn replaces_all_macros() {
        let values = sample_values();
        let template =
            "https://t.example.com/n?a=${AUCTION_ID}&s=${AUCTION_SEAT_ID}&i=${AUCTION_IMP_ID}&p=${AUCTION_PRICE}&c=${AUCTION_CURRENCY}";
        let result = replace_macros(template, &values);
        assert_eq!(
            result,
            "https://t.example.com/n?a=auction-123&s=appnexus&i=imp-456&p=1.5000&c=USD"
        );
    }

    #[test]
    fn apply_bid_macros_replaces_nurl_and_burl() {
        let values = sample_values();
        let mut bid = openrtb::Bid {
            id: "b1".to_string(),
            impid: "imp-456".to_string(),
            price: 1.5,
            nurl: Some("https://n.example.com/win?p=${AUCTION_PRICE}".to_string()),
            burl: Some("https://b.example.com/bill?p=${AUCTION_PRICE}".to_string()),
            adm: Some("<div>creative</div>".to_string()),
            ..Default::default()
        };
        apply_bid_macros(&mut bid, &values);
        assert_eq!(
            bid.nurl.as_deref(),
            Some("https://n.example.com/win?p=1.5000")
        );
        assert_eq!(
            bid.burl.as_deref(),
            Some("https://b.example.com/bill?p=1.5000")
        );
        // adm has no macro, should be unchanged
        assert_eq!(bid.adm.as_deref(), Some("<div>creative</div>"));
    }

    #[test]
    fn apply_bid_macros_replaces_adm_when_macro_present() {
        let values = sample_values();
        let mut bid = openrtb::Bid {
            id: "b2".to_string(),
            impid: "imp-456".to_string(),
            price: 1.5,
            adm: Some("<script>var p='${AUCTION_PRICE}'</script>".to_string()),
            ..Default::default()
        };
        apply_bid_macros(&mut bid, &values);
        assert_eq!(
            bid.adm.as_deref(),
            Some("<script>var p='1.5000'</script>")
        );
    }
}
