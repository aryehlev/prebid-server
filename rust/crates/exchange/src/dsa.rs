/// Digital Services Act (DSA) transparency validation and request writing.
///
/// This module mirrors the Go `dsa` package, providing:
/// - Types for DSA request/response objects per the IAB OpenRTB DSA extension.
/// - Validation of bid responses against DSA requirements.
/// - A `DsaWriter` that applies default DSA signals to bid requests.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Constants ────────────────────────────────────────────────────────────────

/// DSA `dsarequired` values.
pub mod required {
    /// Bid responses without a DSA object will not be accepted.
    pub const REQUIRED: i8 = 2;
    /// Bid responses without a DSA object will not be accepted; publisher is an online platform.
    pub const REQUIRED_ONLINE_PLATFORM: i8 = 3;
}

/// DSA `pubrender` values.
pub mod pub_render {
    /// Publisher cannot render.
    pub const CANNOT_RENDER: i8 = 0;
    /// Publisher will render.
    pub const WILL_RENDER: i8 = 2;
}

/// DSA `adrender` values.
pub mod ad_render {
    /// Buyer/advertiser will render.
    pub const WILL_RENDER: i8 = 1;
}

const BEHALF_MAX_LENGTH: usize = 100;
const PAID_MAX_LENGTH: usize = 100;

// ── Error Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq)]
pub enum DsaError {
    #[error("DSA object missing when required")]
    DsaMissing,
    #[error("DSA behalf exceeds limit of 100 chars")]
    BehalfTooLong,
    #[error("DSA paid exceeds limit of 100 chars")]
    PaidTooLong,
    #[error("DSA publisher and buyer both signal will not render")]
    NeitherWillRender,
    #[error("DSA publisher and buyer both signal will render")]
    BothWillRender,
}

// ── DSA Request Types ────────────────────────────────────────────────────────

/// DSA transparency entry within a DSA request object.
///
/// Corresponds to `regs.ext.dsa.transparency[i]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DsaTransparency {
    /// The domain of the entity providing transparency information.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub domain: String,
    /// DSA transparency parameters.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dsaparams: Vec<i32>,
}

/// DSA object on the bid request (`regs.ext.dsa`).
///
/// Specifies the publisher's DSA requirements for bid responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DsaRequest {
    /// Whether a DSA response object is required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dsarequired: Option<i8>,
    /// Publisher rendering intentions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pubrender: Option<i8>,
    /// Transparency data to publisher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datatopub: Option<i8>,
    /// Transparency entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transparency: Vec<DsaTransparency>,
}

/// DSA object on a bid response (`seatbid.bid[].ext.dsa`).
///
/// Carries DSA transparency information from the buyer/advertiser.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DsaResponse {
    /// On whose behalf the ad is displayed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub behalf: String,
    /// Who paid for the ad.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub paid: String,
    /// Transparency entries from the buyer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transparency: Vec<DsaTransparency>,
    /// Buyer/advertiser rendering intentions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adrender: Option<i8>,
}

// ── Validation ───────────────────────────────────────────────────────────────

/// Validate a bid's DSA response against the request's DSA requirements.
///
/// A bid is invalid if:
/// - The request requires DSA (dsarequired >= 2) and the response has no DSA object.
/// - The `behalf` field exceeds 100 characters.
/// - The `paid` field exceeds 100 characters.
/// - Both publisher and buyer signal they will not render (pubrender=0 and adrender!=1).
/// - Both publisher and buyer signal they will render (pubrender=2 and adrender=1).
pub fn validate_dsa(
    request_dsa: Option<&DsaRequest>,
    response_dsa: Option<&DsaResponse>,
) -> Result<(), DsaError> {
    if dsa_required(request_dsa) && response_dsa.is_none() {
        return Err(DsaError::DsaMissing);
    }

    let bid_dsa = match response_dsa {
        Some(d) => d,
        None => return Ok(()),
    };

    if bid_dsa.behalf.len() > BEHALF_MAX_LENGTH {
        return Err(DsaError::BehalfTooLong);
    }
    if bid_dsa.paid.len() > PAID_MAX_LENGTH {
        return Err(DsaError::PaidTooLong);
    }

    if let Some(req_dsa) = request_dsa {
        if let (Some(pub_render), Some(ad_render)) = (req_dsa.pubrender, bid_dsa.adrender) {
            if pub_render == pub_render::CANNOT_RENDER && ad_render != ad_render::WILL_RENDER {
                return Err(DsaError::NeitherWillRender);
            }
            if pub_render == pub_render::WILL_RENDER && ad_render == ad_render::WILL_RENDER {
                return Err(DsaError::BothWillRender);
            }
        }
    }

    Ok(())
}

/// Check whether the DSA request indicates that a DSA response is required.
fn dsa_required(request_dsa: Option<&DsaRequest>) -> bool {
    match request_dsa {
        Some(dsa) => match dsa.dsarequired {
            Some(v) => v == required::REQUIRED || v == required::REQUIRED_ONLINE_PLATFORM,
            None => false,
        },
        None => false,
    }
}

// ── DSA Writer ───────────────────────────────────────────────────────────────

/// Configuration for default DSA settings from the account.
#[derive(Debug, Clone, Default)]
pub struct DsaAccountConfig {
    /// Default DSA object to apply when none is present on the request.
    pub default: Option<DsaRequest>,
    /// Only apply the default when GDPR is in scope.
    pub gdpr_only: bool,
}

/// Applies default DSA signals to outgoing bid requests.
///
/// If the request does not already carry a DSA object at `regs.ext.dsa`,
/// and the account has a default configured, the writer inserts it.
/// When `gdpr_only` is set, the default is only applied if GDPR is in scope.
pub struct DsaWriter {
    pub config: Option<DsaAccountConfig>,
    pub gdpr_in_scope: bool,
}

impl DsaWriter {
    /// Write the default DSA object onto a bid request's `regs.ext.dsa` if needed.
    ///
    /// Returns `Ok(true)` if a default was written, `Ok(false)` if nothing was changed.
    pub fn write(&self, request_ext: &mut serde_json::Value) -> Result<bool, String> {
        // If there is already a DSA on the request, do nothing.
        if get_request_dsa(request_ext).is_some() {
            return Ok(false);
        }

        let config = match &self.config {
            Some(c) => c,
            None => return Ok(false),
        };

        let default_dsa = match &config.default {
            Some(d) => d,
            None => return Ok(false),
        };

        if config.gdpr_only && !self.gdpr_in_scope {
            return Ok(false);
        }

        let dsa_value = serde_json::to_value(default_dsa)
            .map_err(|e| format!("failed to serialize DSA default: {}", e))?;

        // Ensure regs.ext exists and set dsa on it.
        let regs = request_ext
            .as_object_mut()
            .ok_or("request ext is not an object")?
            .entry("regs")
            .or_insert_with(|| serde_json::json!({}));
        let regs_ext = regs
            .as_object_mut()
            .ok_or("regs is not an object")?
            .entry("ext")
            .or_insert_with(|| serde_json::json!({}));
        regs_ext
            .as_object_mut()
            .ok_or("regs.ext is not an object")?
            .insert("dsa".to_string(), dsa_value);

        Ok(true)
    }
}

/// Extract the DSA request object from `regs.ext.dsa` in a request ext value.
pub fn get_request_dsa(ext: &serde_json::Value) -> Option<DsaRequest> {
    ext.get("regs")
        .and_then(|r| r.get("ext"))
        .and_then(|e| e.get("dsa"))
        .and_then(|d| serde_json::from_value::<DsaRequest>(d.clone()).ok())
}

/// Extract the DSA response object from a bid's `ext.dsa`.
pub fn get_bid_dsa(bid_ext: &serde_json::Value) -> Option<DsaResponse> {
    bid_ext
        .get("dsa")
        .and_then(|d| serde_json::from_value::<DsaResponse>(d.clone()).ok())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_dsa_not_required_no_response() {
        let req = DsaRequest {
            dsarequired: Some(1),
            ..Default::default()
        };
        assert!(validate_dsa(Some(&req), None).is_ok());
    }

    #[test]
    fn test_validate_dsa_required_missing_response() {
        let req = DsaRequest {
            dsarequired: Some(2),
            ..Default::default()
        };
        assert_eq!(validate_dsa(Some(&req), None), Err(DsaError::DsaMissing));
    }

    #[test]
    fn test_validate_dsa_required_online_platform_missing() {
        let req = DsaRequest {
            dsarequired: Some(3),
            ..Default::default()
        };
        assert_eq!(validate_dsa(Some(&req), None), Err(DsaError::DsaMissing));
    }

    #[test]
    fn test_validate_dsa_required_with_response() {
        let req = DsaRequest {
            dsarequired: Some(2),
            ..Default::default()
        };
        let resp = DsaResponse {
            behalf: "adv".to_string(),
            paid: "adv".to_string(),
            ..Default::default()
        };
        assert!(validate_dsa(Some(&req), Some(&resp)).is_ok());
    }

    #[test]
    fn test_validate_behalf_too_long() {
        let resp = DsaResponse {
            behalf: "a".repeat(101),
            ..Default::default()
        };
        assert_eq!(
            validate_dsa(None, Some(&resp)),
            Err(DsaError::BehalfTooLong)
        );
    }

    #[test]
    fn test_validate_paid_too_long() {
        let resp = DsaResponse {
            paid: "a".repeat(101),
            ..Default::default()
        };
        assert_eq!(
            validate_dsa(None, Some(&resp)),
            Err(DsaError::PaidTooLong)
        );
    }

    #[test]
    fn test_validate_neither_will_render() {
        let req = DsaRequest {
            pubrender: Some(pub_render::CANNOT_RENDER),
            ..Default::default()
        };
        let resp = DsaResponse {
            adrender: Some(0), // not ad_render::WILL_RENDER
            ..Default::default()
        };
        assert_eq!(
            validate_dsa(Some(&req), Some(&resp)),
            Err(DsaError::NeitherWillRender)
        );
    }

    #[test]
    fn test_validate_both_will_render() {
        let req = DsaRequest {
            pubrender: Some(pub_render::WILL_RENDER),
            ..Default::default()
        };
        let resp = DsaResponse {
            adrender: Some(ad_render::WILL_RENDER),
            ..Default::default()
        };
        assert_eq!(
            validate_dsa(Some(&req), Some(&resp)),
            Err(DsaError::BothWillRender)
        );
    }

    #[test]
    fn test_validate_compatible_render() {
        // pubrender=0 (cannot render), adrender=1 (will render) => valid
        let req = DsaRequest {
            pubrender: Some(pub_render::CANNOT_RENDER),
            ..Default::default()
        };
        let resp = DsaResponse {
            adrender: Some(ad_render::WILL_RENDER),
            ..Default::default()
        };
        assert!(validate_dsa(Some(&req), Some(&resp)).is_ok());
    }

    #[test]
    fn test_dsa_writer_no_config() {
        let writer = DsaWriter {
            config: None,
            gdpr_in_scope: true,
        };
        let mut ext = serde_json::json!({});
        assert_eq!(writer.write(&mut ext).unwrap(), false);
    }

    #[test]
    fn test_dsa_writer_already_present() {
        let writer = DsaWriter {
            config: Some(DsaAccountConfig {
                default: Some(DsaRequest {
                    dsarequired: Some(2),
                    ..Default::default()
                }),
                gdpr_only: false,
            }),
            gdpr_in_scope: true,
        };
        let mut ext = serde_json::json!({
            "regs": { "ext": { "dsa": { "dsarequired": 1 } } }
        });
        assert_eq!(writer.write(&mut ext).unwrap(), false);
    }

    #[test]
    fn test_dsa_writer_applies_default() {
        let writer = DsaWriter {
            config: Some(DsaAccountConfig {
                default: Some(DsaRequest {
                    dsarequired: Some(2),
                    pubrender: Some(0),
                    ..Default::default()
                }),
                gdpr_only: false,
            }),
            gdpr_in_scope: false,
        };
        let mut ext = serde_json::json!({});
        assert_eq!(writer.write(&mut ext).unwrap(), true);
        let dsa = ext
            .get("regs")
            .unwrap()
            .get("ext")
            .unwrap()
            .get("dsa")
            .unwrap();
        assert_eq!(dsa.get("dsarequired").unwrap(), 2);
    }

    #[test]
    fn test_dsa_writer_gdpr_only_not_in_scope() {
        let writer = DsaWriter {
            config: Some(DsaAccountConfig {
                default: Some(DsaRequest {
                    dsarequired: Some(2),
                    ..Default::default()
                }),
                gdpr_only: true,
            }),
            gdpr_in_scope: false,
        };
        let mut ext = serde_json::json!({});
        assert_eq!(writer.write(&mut ext).unwrap(), false);
    }

    #[test]
    fn test_dsa_writer_gdpr_only_in_scope() {
        let writer = DsaWriter {
            config: Some(DsaAccountConfig {
                default: Some(DsaRequest {
                    dsarequired: Some(2),
                    ..Default::default()
                }),
                gdpr_only: true,
            }),
            gdpr_in_scope: true,
        };
        let mut ext = serde_json::json!({});
        assert_eq!(writer.write(&mut ext).unwrap(), true);
    }

    #[test]
    fn test_get_request_dsa() {
        let ext = serde_json::json!({
            "regs": {
                "ext": {
                    "dsa": {
                        "dsarequired": 2,
                        "pubrender": 0,
                        "datatopub": 1,
                        "transparency": [
                            { "domain": "example.com", "dsaparams": [1, 2] }
                        ]
                    }
                }
            }
        });
        let dsa = get_request_dsa(&ext).unwrap();
        assert_eq!(dsa.dsarequired, Some(2));
        assert_eq!(dsa.pubrender, Some(0));
        assert_eq!(dsa.datatopub, Some(1));
        assert_eq!(dsa.transparency.len(), 1);
        assert_eq!(dsa.transparency[0].domain, "example.com");
        assert_eq!(dsa.transparency[0].dsaparams, vec![1, 2]);
    }

    #[test]
    fn test_get_bid_dsa() {
        let ext = serde_json::json!({
            "dsa": {
                "behalf": "advertiser",
                "paid": "advertiser",
                "adrender": 1,
                "transparency": [
                    { "domain": "buyer.com", "dsaparams": [3] }
                ]
            }
        });
        let dsa = get_bid_dsa(&ext).unwrap();
        assert_eq!(dsa.behalf, "advertiser");
        assert_eq!(dsa.paid, "advertiser");
        assert_eq!(dsa.adrender, Some(1));
        assert_eq!(dsa.transparency.len(), 1);
    }

    #[test]
    fn test_no_request_no_response() {
        assert!(validate_dsa(None, None).is_ok());
    }
}
