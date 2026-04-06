//! AMP (Accelerated Mobile Pages) request parsing.
//! Mirrors Go `amp/parse.go`.
//!
//! Parses AMP endpoint query parameters into structured parameters used
//! for building bid requests.

/// AMP request parameters parsed from query string.
#[derive(Debug, Clone, Default)]
pub struct AmpParams {
    pub account: String,
    pub additional_consent: String,
    pub canonical_url: String,
    pub consent: String,
    pub consent_type: i64,
    pub debug: bool,
    pub gdpr_applies: Option<bool>,
    pub origin: String,
    pub size: AmpSize,
    pub slot: String,
    pub stored_request_id: String,
    pub targeting: String,
    pub timeout: Option<u64>,
    pub trace: String,
}

/// Size information from an AMP request.
#[derive(Debug, Clone, Default)]
pub struct AmpSize {
    pub height: i64,
    pub multisize: Vec<(i64, i64)>,
    pub override_height: i64,
    pub override_width: i64,
    pub width: i64,
}

/// Consent type constants.
pub const CONSENT_NONE: i64 = 0;
pub const CONSENT_TCF1: i64 = 1;
pub const CONSENT_TCF2: i64 = 2;
pub const CONSENT_US_PRIVACY: i64 = 3;

/// Parse AMP parameters from a query string map.
/// Mirrors Go `ParseParams`.
pub fn parse_params(
    query: &std::collections::HashMap<String, String>,
) -> Result<AmpParams, String> {
    let tag_id = query.get("tag_id").map(|s| s.as_str()).unwrap_or("");
    if tag_id.is_empty() {
        return Err("AMP requests require an AMP tag_id".to_string());
    }

    let consent = choose_consent(
        query.get("consent_string").map(|s| s.as_str()).unwrap_or(""),
        query.get("gdpr_consent").map(|s| s.as_str()).unwrap_or(""),
    );

    let mut params = AmpParams {
        account: query
            .get("account")
            .cloned()
            .unwrap_or_default(),
        additional_consent: query
            .get("addtl_consent")
            .cloned()
            .unwrap_or_default(),
        canonical_url: query.get("curl").cloned().unwrap_or_default(),
        consent,
        consent_type: parse_int(query.get("consent_type").map(|s| s.as_str()).unwrap_or("")),
        debug: query.get("debug").map(|s| s.as_str()) == Some("1"),
        gdpr_applies: None,
        origin: query
            .get("__amp_source_origin")
            .cloned()
            .unwrap_or_default(),
        size: AmpSize {
            height: parse_int(query.get("h").map(|s| s.as_str()).unwrap_or("")),
            multisize: parse_multisize(query.get("ms").map(|s| s.as_str()).unwrap_or("")),
            override_height: parse_int(query.get("oh").map(|s| s.as_str()).unwrap_or("")),
            override_width: parse_int(query.get("ow").map(|s| s.as_str()).unwrap_or("")),
            width: parse_int(query.get("w").map(|s| s.as_str()).unwrap_or("")),
        },
        slot: query.get("slot").cloned().unwrap_or_default(),
        stored_request_id: tag_id.to_string(),
        targeting: query.get("targeting").cloned().unwrap_or_default(),
        timeout: None,
        trace: query.get("trace").cloned().unwrap_or_default(),
    };

    if let Some(gdpr_applies_str) = query.get("gdpr_applies") {
        if !gdpr_applies_str.is_empty() {
            match gdpr_applies_str.parse::<bool>() {
                Ok(v) => params.gdpr_applies = Some(v),
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    if let Some(timeout_str) = query.get("timeout") {
        if !timeout_str.is_empty() {
            match timeout_str.parse::<u64>() {
                Ok(v) => params.timeout = Some(v),
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    Ok(params)
}

fn parse_int(value: &str) -> i64 {
    value.parse::<i64>().unwrap_or(0)
}

fn parse_multisize(multisize: &str) -> Vec<(i64, i64)> {
    if multisize.is_empty() {
        return Vec::new();
    }

    let mut sizes = Vec::new();
    for size_str in multisize.split(',') {
        let parts: Vec<&str> = size_str.split('x').collect();
        if parts.len() != 2 {
            return Vec::new();
        }
        let w = parse_int(parts[0]);
        let h = parse_int(parts[1]);
        if w == 0 && h == 0 {
            return Vec::new();
        }
        sizes.push((w, h));
    }
    sizes
}

fn choose_consent(consent: &str, gdpr_consent: &str) -> String {
    if !consent.is_empty() {
        return consent.to_string();
    }
    gdpr_consent.to_string()
}

/// Parse gdpr_applies boolean to integer (0 or 1).
pub fn parse_gdpr_applies(gdpr_applies: Option<bool>) -> i8 {
    match gdpr_applies {
        Some(true) => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_query(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_parse_params_missing_tag_id() {
        let query = make_query(&[("account", "123")]);
        assert!(parse_params(&query).is_err());
    }

    #[test]
    fn test_parse_params_basic() {
        let query = make_query(&[
            ("tag_id", "my-tag"),
            ("account", "acct-123"),
            ("w", "300"),
            ("h", "250"),
            ("debug", "1"),
        ]);
        let params = parse_params(&query).unwrap();
        assert_eq!(params.stored_request_id, "my-tag");
        assert_eq!(params.account, "acct-123");
        assert_eq!(params.size.width, 300);
        assert_eq!(params.size.height, 250);
        assert!(params.debug);
    }

    #[test]
    fn test_parse_params_consent() {
        let query = make_query(&[
            ("tag_id", "t"),
            ("consent_string", "my-consent"),
            ("consent_type", "2"),
        ]);
        let params = parse_params(&query).unwrap();
        assert_eq!(params.consent, "my-consent");
        assert_eq!(params.consent_type, CONSENT_TCF2);
    }

    #[test]
    fn test_parse_params_gdpr_consent_fallback() {
        let query = make_query(&[("tag_id", "t"), ("gdpr_consent", "fallback")]);
        let params = parse_params(&query).unwrap();
        assert_eq!(params.consent, "fallback");
    }

    #[test]
    fn test_parse_multisize() {
        let sizes = parse_multisize("300x250,728x90");
        assert_eq!(sizes, vec![(300, 250), (728, 90)]);
    }

    #[test]
    fn test_parse_multisize_empty() {
        assert!(parse_multisize("").is_empty());
    }

    #[test]
    fn test_parse_multisize_invalid() {
        assert!(parse_multisize("300x250,bad").is_empty());
    }

    #[test]
    fn test_parse_gdpr_applies() {
        assert_eq!(parse_gdpr_applies(None), 0);
        assert_eq!(parse_gdpr_applies(Some(false)), 0);
        assert_eq!(parse_gdpr_applies(Some(true)), 1);
    }

    #[test]
    fn test_parse_params_timeout() {
        let query = make_query(&[("tag_id", "t"), ("timeout", "500")]);
        let params = parse_params(&query).unwrap();
        assert_eq!(params.timeout, Some(500));
    }

    #[test]
    fn test_parse_params_gdpr_applies() {
        let query = make_query(&[("tag_id", "t"), ("gdpr_applies", "true")]);
        let params = parse_params(&query).unwrap();
        assert_eq!(params.gdpr_applies, Some(true));
    }
}
