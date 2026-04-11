//! CCPA (California Consumer Privacy Act) enforcement.
//!
//! Mirrors Go `privacy/ccpa/` package — parses CCPA consent strings,
//! manages no-sale bidder lists, and enforces opt-out.

use std::collections::HashSet;

// CCPA consent string constants
const CCPA_VERSION_1: u8 = b'1';
const CCPA_YES: u8 = b'Y';
const CCPA_NO: u8 = b'N';
const CCPA_NOT_APPLICABLE: u8 = b'-';

const INDEX_VERSION: usize = 0;
const INDEX_EXPLICIT_NOTICE: usize = 1;
const INDEX_OPT_OUT_SALE: usize = 2;
const INDEX_LSPA_COVERED: usize = 3;

const ALL_BIDDERS_MARKER: &str = "*";

/// Policy represents the CCPA regulatory information from an OpenRTB bid request.
/// Mirrors Go `ccpa.Policy`.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    pub consent: String,
    pub no_sale_bidders: Vec<String>,
}

/// ParsedPolicy represents parsed and validated CCPA information for enforcement.
/// Mirrors Go `ccpa.ParsedPolicy`.
#[derive(Debug, Clone, Default)]
pub struct ParsedPolicy {
    consent_specified: bool,
    consent_opt_out_sale: bool,
    no_sale_for_all_bidders: bool,
    no_sale_specific_bidders: HashSet<String>,
}

impl Policy {
    /// Parse returns a parsed and validated ParsedPolicy for enforcement decisions.
    /// Mirrors Go `Policy.Parse`.
    pub fn parse(&self, valid_bidders: &HashSet<String>) -> Result<ParsedPolicy, String> {
        let consent_opt_out = parse_consent(&self.consent)?;

        let (no_sale_for_all, no_sale_specific) =
            parse_no_sale_bidders(&self.no_sale_bidders, valid_bidders)?;

        Ok(ParsedPolicy {
            consent_specified: !self.consent.is_empty(),
            consent_opt_out_sale: consent_opt_out,
            no_sale_for_all_bidders: no_sale_for_all,
            no_sale_specific_bidders: no_sale_specific,
        })
    }
}

impl ParsedPolicy {
    /// CanEnforce returns true when consent is specifically provided.
    pub fn can_enforce(&self) -> bool {
        self.consent_specified
    }

    /// ShouldEnforce returns true when the opt-out signal is detected for a bidder.
    /// Mirrors Go `ParsedPolicy.ShouldEnforce`.
    pub fn should_enforce(&self, bidder: &str) -> bool {
        !self.is_no_sale_for_bidder(bidder) && self.consent_opt_out_sale
    }

    fn is_no_sale_for_bidder(&self, bidder: &str) -> bool {
        if self.no_sale_for_all_bidders {
            return true;
        }
        self.no_sale_specific_bidders.contains(bidder)
    }
}

impl super::PolicyEnforcer for ParsedPolicy {
    fn can_enforce(&self) -> bool {
        self.consent_specified
    }
    fn should_enforce(&self, bidder: &str) -> bool {
        self.should_enforce(bidder)
    }
}

/// Validate a CCPA consent string.
/// Returns true if the consent string is empty or valid per IAB spec.
pub fn validate_consent(consent: &str) -> bool {
    parse_consent(consent).is_ok()
}

/// Parse a CCPA consent string. Returns whether opt-out-sale is set.
/// Mirrors Go `parseConsent`.
fn parse_consent(consent: &str) -> Result<bool, String> {
    if consent.is_empty() {
        return Ok(false);
    }

    let bytes = consent.as_bytes();
    if bytes.len() != 4 {
        return Err("must contain 4 characters".to_string());
    }

    if bytes[INDEX_VERSION] != CCPA_VERSION_1 {
        return Err("must specify version 1".to_string());
    }

    let c = bytes[INDEX_EXPLICIT_NOTICE];
    if c != CCPA_NO && c != CCPA_YES && c != CCPA_NOT_APPLICABLE {
        return Err("must specify 'N', 'Y', or '-' for the explicit notice".to_string());
    }

    let c = bytes[INDEX_OPT_OUT_SALE];
    if c != CCPA_NO && c != CCPA_YES && c != CCPA_NOT_APPLICABLE {
        return Err("must specify 'N', 'Y', or '-' for the opt-out sale".to_string());
    }

    let c = bytes[INDEX_LSPA_COVERED];
    if c != CCPA_NO && c != CCPA_YES && c != CCPA_NOT_APPLICABLE {
        return Err("must specify 'N', 'Y', or '-' for the limited service provider agreement".to_string());
    }

    Ok(bytes[INDEX_OPT_OUT_SALE] == CCPA_YES)
}

/// Parse the no-sale bidder list.
fn parse_no_sale_bidders(
    no_sale_bidders: &[String],
    valid_bidders: &HashSet<String>,
) -> Result<(bool, HashSet<String>), String> {
    let mut specific = HashSet::new();

    if no_sale_bidders.len() == 1 && no_sale_bidders[0] == ALL_BIDDERS_MARKER {
        return Ok((true, specific));
    }

    for bidder in no_sale_bidders {
        if bidder == ALL_BIDDERS_MARKER {
            return Err("can only specify all bidders if no other bidders are provided".to_string());
        }
        if valid_bidders.contains(bidder) {
            specific.insert(bidder.clone());
        } else {
            return Err(format!("unrecognized bidder '{}'", bidder));
        }
    }

    Ok((false, specific))
}

/// ConsentWriter writes CCPA consent to a bid request.
/// Mirrors Go `ccpa.ConsentWriter`.
pub struct ConsentWriter {
    pub consent: String,
}

impl super::PolicyWriter for ConsentWriter {
    fn write(&self, req: &mut openrtb::BidRequest) -> Result<(), String> {
        if !self.consent.is_empty() {
            if req.regs.is_none() {
                req.regs = Some(openrtb::Regs::default());
            }
            if let Some(ref mut regs) = req.regs {
                regs.us_privacy = Some(self.consent.clone());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_consent_empty() {
        assert!(validate_consent(""));
    }

    #[test]
    fn test_validate_consent_valid() {
        assert!(validate_consent("1YNN"));
        assert!(validate_consent("1NYN"));
        assert!(validate_consent("1---"));
        assert!(validate_consent("1Y-N"));
    }

    #[test]
    fn test_validate_consent_invalid_length() {
        assert!(!validate_consent("1YN"));
        assert!(!validate_consent("1YNNN"));
    }

    #[test]
    fn test_validate_consent_invalid_version() {
        assert!(!validate_consent("2YNN"));
    }

    #[test]
    fn test_validate_consent_invalid_chars() {
        assert!(!validate_consent("1XNN"));
        assert!(!validate_consent("1YXN"));
        assert!(!validate_consent("1YNX"));
    }

    #[test]
    fn test_parse_consent_opt_out() {
        assert!(parse_consent("1YYN").unwrap());
        assert!(!parse_consent("1YNN").unwrap());
        assert!(!parse_consent("1Y-N").unwrap());
    }

    #[test]
    fn test_parsed_policy_should_enforce() {
        let policy = ParsedPolicy {
            consent_specified: true,
            consent_opt_out_sale: true,
            no_sale_for_all_bidders: false,
            no_sale_specific_bidders: HashSet::new(),
        };
        assert!(policy.should_enforce("appnexus"));
    }

    #[test]
    fn test_parsed_policy_no_sale_exempts_bidder() {
        let mut no_sale = HashSet::new();
        no_sale.insert("appnexus".to_string());
        let policy = ParsedPolicy {
            consent_specified: true,
            consent_opt_out_sale: true,
            no_sale_for_all_bidders: false,
            no_sale_specific_bidders: no_sale,
        };
        // appnexus is in no-sale list, so should NOT enforce
        assert!(!policy.should_enforce("appnexus"));
        // rubicon is NOT in no-sale list, so should enforce
        assert!(policy.should_enforce("rubicon"));
    }

    #[test]
    fn test_parsed_policy_no_sale_all() {
        let policy = ParsedPolicy {
            consent_specified: true,
            consent_opt_out_sale: true,
            no_sale_for_all_bidders: true,
            no_sale_specific_bidders: HashSet::new(),
        };
        assert!(!policy.should_enforce("anyone"));
    }

    #[test]
    fn test_policy_parse() {
        let mut valid = HashSet::new();
        valid.insert("appnexus".to_string());
        valid.insert("rubicon".to_string());

        let policy = Policy {
            consent: "1YYN".to_string(),
            no_sale_bidders: vec!["appnexus".to_string()],
        };
        let parsed = policy.parse(&valid).unwrap();
        assert!(parsed.can_enforce());
        assert!(!parsed.should_enforce("appnexus"));
        assert!(parsed.should_enforce("rubicon"));
    }

    #[test]
    fn test_no_sale_all_bidders() {
        let valid = HashSet::new();
        let policy = Policy {
            consent: "1YYN".to_string(),
            no_sale_bidders: vec!["*".to_string()],
        };
        let parsed = policy.parse(&valid).unwrap();
        assert!(!parsed.should_enforce("anyone"));
    }
}
