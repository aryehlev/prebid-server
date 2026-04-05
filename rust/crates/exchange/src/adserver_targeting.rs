use std::collections::HashMap;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct AdServerTargetingRule {
    pub key: String,
    pub source: String,  // "bidrequest", "bidresponse", "static"
    pub value: String,   // JSONPath-like: "site.page", "seatbid.0.bid.0.price", or literal
}

/// Apply ad server targeting rules from req.ext.prebid.adservertargeting
/// Returns additional key-value pairs to merge into the targeting map
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
