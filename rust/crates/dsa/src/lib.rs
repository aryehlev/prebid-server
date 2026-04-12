//! Digital Services Act (DSA) validation, ported from the Go `dsa`
//! package. Self-contained: does not depend on the openrtb crates so
//! the exchange can adapt its own types to [`DsaRequest`] /
//! [`DsaTransparency`] when invoking [`validate_dsa`].

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Required values (matches Go constants).
pub const REQUIRED: i8 = 2;
pub const REQUIRED_ONLINE_PLATFORM: i8 = 3;

/// PubRender values describing publisher rendering intent.
pub const PUB_RENDER_CANNOT_RENDER: i8 = 0;
pub const PUB_RENDER_WILL_RENDER: i8 = 2;

/// AdRender values describing advertiser/buyer rendering intent.
pub const AD_RENDER_WILL_RENDER: i8 = 1;

const BEHALF_MAX_LENGTH: usize = 100;
const PAID_MAX_LENGTH: usize = 100;

/// Errors raised by DSA validation.
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

/// DSA request object (mirrors `ExtRegsDSA`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DsaRequest {
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "dsarequired")]
    pub required: Option<i8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pub_render: Option<i8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_to_pub: Option<i8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transparency: Vec<DsaTransparency>,
}

/// DSA transparency entry (mirrors `ExtBidDSATransparency`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DsaTransparency {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub domain: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "dsaparams")]
    pub params: Vec<i32>,
}

/// DSA response object on a bid (mirrors `ExtBidDSA`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DsaResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ad_render: Option<i8>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub behalf: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub paid: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transparency: Vec<DsaTransparency>,
}

/// Validate a bid DSA response against the request-level DSA rules.
///
/// A bid is considered valid unless the bid request indicates that a
/// DSA object is required and it is missing, or the bid's DSA object
/// contents are invalid.
pub fn validate_dsa(req: Option<&DsaRequest>, bid: Option<&DsaResponse>) -> Result<(), DsaError> {
    if dsa_required(req) && bid.is_none() {
        return Err(DsaError::DsaMissing);
    }
    let Some(bid) = bid else {
        return Ok(());
    };
    if bid.behalf.len() > BEHALF_MAX_LENGTH {
        return Err(DsaError::BehalfTooLong);
    }
    if bid.paid.len() > PAID_MAX_LENGTH {
        return Err(DsaError::PaidTooLong);
    }
    if let (Some(req), Some(ad)) = (req, bid.ad_render) {
        if let Some(pub_render) = req.pub_render {
            if pub_render == PUB_RENDER_CANNOT_RENDER && ad != AD_RENDER_WILL_RENDER {
                return Err(DsaError::NeitherWillRender);
            }
            if pub_render == PUB_RENDER_WILL_RENDER && ad == AD_RENDER_WILL_RENDER {
                return Err(DsaError::BothWillRender);
            }
        }
    }
    Ok(())
}

fn dsa_required(req: Option<&DsaRequest>) -> bool {
    match req.and_then(|r| r.required) {
        Some(v) => v == REQUIRED || v == REQUIRED_ONLINE_PLATFORM,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_missing_when_required() {
        let req = DsaRequest {
            required: Some(REQUIRED),
            ..Default::default()
        };
        assert_eq!(validate_dsa(Some(&req), None), Err(DsaError::DsaMissing));
    }

    #[test]
    fn validate_ok_when_not_required_and_missing() {
        let req = DsaRequest::default();
        assert!(validate_dsa(Some(&req), None).is_ok());
    }

    #[test]
    fn validate_behalf_too_long() {
        let bid = DsaResponse {
            behalf: "a".repeat(101),
            ..Default::default()
        };
        assert_eq!(validate_dsa(None, Some(&bid)), Err(DsaError::BehalfTooLong));
    }

    #[test]
    fn validate_paid_too_long() {
        let bid = DsaResponse {
            paid: "b".repeat(200),
            ..Default::default()
        };
        assert_eq!(validate_dsa(None, Some(&bid)), Err(DsaError::PaidTooLong));
    }

    #[test]
    fn validate_neither_will_render() {
        let req = DsaRequest {
            pub_render: Some(PUB_RENDER_CANNOT_RENDER),
            ..Default::default()
        };
        let bid = DsaResponse {
            ad_render: Some(0),
            ..Default::default()
        };
        assert_eq!(
            validate_dsa(Some(&req), Some(&bid)),
            Err(DsaError::NeitherWillRender)
        );
    }

    #[test]
    fn validate_both_will_render() {
        let req = DsaRequest {
            pub_render: Some(PUB_RENDER_WILL_RENDER),
            ..Default::default()
        };
        let bid = DsaResponse {
            ad_render: Some(AD_RENDER_WILL_RENDER),
            ..Default::default()
        };
        assert_eq!(
            validate_dsa(Some(&req), Some(&bid)),
            Err(DsaError::BothWillRender)
        );
    }

    #[test]
    fn validate_valid() {
        let req = DsaRequest {
            required: Some(REQUIRED),
            pub_render: Some(PUB_RENDER_CANNOT_RENDER),
            ..Default::default()
        };
        let bid = DsaResponse {
            ad_render: Some(AD_RENDER_WILL_RENDER),
            behalf: "example".into(),
            paid: "example".into(),
            ..Default::default()
        };
        assert!(validate_dsa(Some(&req), Some(&bid)).is_ok());
    }

    #[test]
    fn parse_dsa_request_json() {
        let json = r#"{"dsarequired": 2, "pub_render": 0, "transparency": [{"domain":"a.com","dsaparams":[1,2]}]}"#;
        let req: DsaRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.required, Some(2));
        assert_eq!(req.transparency.len(), 1);
        assert_eq!(req.transparency[0].params, vec![1, 2]);
    }
}
