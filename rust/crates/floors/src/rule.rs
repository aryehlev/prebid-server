//! Rule matching utilities.
//!
//! Mirrors a slim subset of `floors/rule.go` — enough to build and
//! match rule keys without pulling in openrtb types.

use std::collections::BTreeMap;

use crate::types::{PriceFloorSchema, CATCH_ALL, DEFAULT_DELIMITER};

/// Context values used to build a rule key. Any dimension that should
/// participate in matching must be filled in by the caller from their
/// own request/imp type.
#[derive(Debug, Clone, Default)]
pub struct RuleContext {
    pub site_domain: Option<String>,
    pub pub_domain: Option<String>,
    pub domain: Option<String>,
    pub bundle: Option<String>,
    pub channel: Option<String>,
    pub media_type: Option<String>,
    pub size: Option<String>,
    pub gpt_slot: Option<String>,
    pub ad_unit_code: Option<String>,
    pub country: Option<String>,
    pub device_type: Option<String>,
}

/// Round a float to four decimal places (matching the Go helper).
pub fn round_to_four_decimals(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// Build the desired rule key for a given schema and context.
pub fn create_rule_key(schema: &PriceFloorSchema, ctx: &RuleContext) -> String {
    let delim = if schema.delimiter.is_empty() {
        DEFAULT_DELIMITER
    } else {
        schema.delimiter.as_str()
    };
    let parts: Vec<String> = schema
        .fields
        .iter()
        .map(|field| lookup_dimension(field, ctx).unwrap_or_else(|| CATCH_ALL.to_string()))
        .collect();
    parts.join(delim)
}

fn lookup_dimension(field: &str, ctx: &RuleContext) -> Option<String> {
    match field {
        "siteDomain" => ctx.site_domain.clone(),
        "pubDomain" => ctx.pub_domain.clone(),
        "domain" => ctx.domain.clone(),
        "bundle" => ctx.bundle.clone(),
        "channel" => ctx.channel.clone(),
        "mediaType" => ctx.media_type.clone(),
        "size" => ctx.size.clone(),
        "gptSlot" => ctx.gpt_slot.clone(),
        "adUnitCode" => ctx.ad_unit_code.clone(),
        "country" => ctx.country.clone(),
        "deviceType" => ctx.device_type.clone(),
        _ => None,
    }
}

/// Find the best matching rule in `values` for `desired_key`. Returns
/// the matched rule key and a flag indicating whether a match was
/// found. This is a simplified port that only checks for an exact
/// match; wildcard-aware matching is left as a follow-up.
pub fn find_rule<'a>(
    values: &'a BTreeMap<String, f64>,
    _delimiter: &str,
    desired_key: &str,
) -> (Option<&'a str>, bool) {
    let key = desired_key.to_lowercase();
    if values.contains_key(&key) {
        // Re-locate the stored key slice.
        let stored = values.keys().find(|k| k.as_str() == key).map(|k| k.as_str());
        return (stored, true);
    }
    (None, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_to_four_decimals_rounds() {
        assert_eq!(round_to_four_decimals(1.234567), 1.2346);
        assert_eq!(round_to_four_decimals(0.0), 0.0);
    }

    #[test]
    fn create_rule_key_uses_context_and_wildcard() {
        let schema = PriceFloorSchema {
            fields: vec!["mediaType".into(), "country".into(), "deviceType".into()],
            delimiter: "|".into(),
        };
        let ctx = RuleContext {
            media_type: Some("banner".into()),
            country: Some("US".into()),
            ..Default::default()
        };
        assert_eq!(create_rule_key(&schema, &ctx), "banner|US|*");
    }

    #[test]
    fn find_rule_exact_match() {
        let mut values = BTreeMap::new();
        values.insert("banner|us".to_string(), 1.0);
        let (matched, found) = find_rule(&values, "|", "Banner|US");
        assert!(found);
        assert_eq!(matched, Some("banner|us"));
    }
}
