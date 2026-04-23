//! High-level privacy policy enforcement over raw JSON bid requests.
//!
//! `PolicyEnforcer` (note: a struct, distinct from the `PolicyEnforcer`
//! trait in `lib.rs`) consolidates GDPR, CCPA, GPP, and COPPA signals
//! and mutates the incoming request accordingly. Where typed
//! scrubbing is desired callers should additionally route through
//! [`crate::scrubber`] on a decoded `BidRequest`.

use serde_json::{json, Value};

use crate::consent_writer::{
    CcpaConsentWriter, ConsentWriter, GppConsentWriter, PrivacyError, TcfConsentWriter,
};

/// Aggregated enforcement inputs for a request.
#[derive(Debug, Clone, Default)]
pub struct PolicyEnforcer;

/// Options passed to [`PolicyEnforcer::enforce`].
#[derive(Debug, Clone, Default)]
pub struct GdprInput {
    pub applies: bool,
    pub consent: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CcpaInput {
    pub us_privacy: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GppInput {
    pub gpp: Option<String>,
    pub gpp_sid: Vec<i32>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CoppaInput {
    pub applies: bool,
}

impl PolicyEnforcer {
    pub fn new() -> Self {
        Self
    }

    /// Enforce privacy signals on the raw JSON request. This applies
    /// consent writers for each provided section and sets COPPA on
    /// `regs.coppa` when applicable. Actual PII removal for the
    /// typed request shape is handled by [`crate::scrubber`]; this
    /// method records the regulatory metadata that downstream typed
    /// scrubbing will act on.
    pub fn enforce(
        &self,
        request: &mut Value,
        gdpr: &GdprInput,
        ccpa: &CcpaInput,
        gpp: &GppInput,
        coppa: &CoppaInput,
    ) -> Result<(), PrivacyError> {
        if !request.is_object() {
            return Err(PrivacyError::InvalidRequest(
                "bid request is not a JSON object".into(),
            ));
        }

        if gdpr.applies {
            if let Some(ref c) = gdpr.consent {
                TcfConsentWriter.write(request, c)?;
            }
        }

        if let Some(ref us) = ccpa.us_privacy {
            CcpaConsentWriter.write(request, us)?;
        }

        if let Some(ref s) = gpp.gpp {
            GppConsentWriter::new(gpp.gpp_sid.clone()).write(request, s)?;
        }

        if coppa.applies {
            let regs = request
                .as_object_mut()
                .unwrap()
                .entry("regs".to_string())
                .or_insert_with(|| json!({}));
            if !regs.is_object() {
                return Err(PrivacyError::InvalidRequest("regs is not an object".into()));
            }
            regs.as_object_mut()
                .unwrap()
                .insert("coppa".into(), json!(1));
        }

        tracing::trace!(
            target: "pbs-privacy::policy_enforcer",
            gdpr = gdpr.applies,
            ccpa = ccpa.us_privacy.is_some(),
            gpp = gpp.gpp.is_some(),
            coppa = coppa.applies,
            "applied privacy policy",
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforce_all_signals() {
        let e = PolicyEnforcer::new();
        let mut req = json!({});
        e.enforce(
            &mut req,
            &GdprInput {
                applies: true,
                consent: Some("CONSENT".into()),
            },
            &CcpaInput {
                us_privacy: Some("1YNN".into()),
            },
            &GppInput {
                gpp: Some("DBABMA~1YNN".into()),
                gpp_sid: vec![6],
            },
            &CoppaInput { applies: true },
        )
        .unwrap();

        assert_eq!(req["regs"]["ext"]["gdpr"], json!(1));
        assert_eq!(req["user"]["ext"]["consent"], json!("CONSENT"));
        assert_eq!(req["regs"]["ext"]["us_privacy"], json!("1YNN"));
        assert_eq!(req["regs"]["gpp"], json!("DBABMA~1YNN"));
        assert_eq!(req["regs"]["gpp_sid"], json!([6]));
        assert_eq!(req["regs"]["coppa"], json!(1));
    }

    #[test]
    fn enforce_noop_when_none_apply() {
        let e = PolicyEnforcer::new();
        let mut req = json!({});
        e.enforce(
            &mut req,
            &GdprInput::default(),
            &CcpaInput::default(),
            &GppInput::default(),
            &CoppaInput::default(),
        )
        .unwrap();
        assert_eq!(req, json!({}));
    }
}
