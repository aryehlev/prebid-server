//! Consent writers mutate a raw `serde_json::Value` bid request in
//! place to add GDPR/CCPA/GPP signals. Mirrors Go
//! `privacy/ConsentWriter` interface implementations used by AMP and
//! stored-request handlers.

use serde_json::{json, Value};
use thiserror::Error;

/// Errors produced by consent writers and privacy orchestration.
#[derive(Debug, Error)]
pub enum PrivacyError {
    #[error("invalid bid request shape: {0}")]
    InvalidRequest(String),
    #[error("consent serialization failed: {0}")]
    Serialize(String),
}

/// A consent writer mutates a raw bid request JSON value.
pub trait ConsentWriter {
    /// Write `consent` into the appropriate field(s) of `request`.
    fn write(&self, request: &mut Value, consent: &str) -> Result<(), PrivacyError>;
}

/// Ensure `request[key]` exists and is an object, returning a mutable
/// reference to it. Returns an error if the slot exists but is not an
/// object.
fn ensure_object<'a>(request: &'a mut Value, key: &str) -> Result<&'a mut Value, PrivacyError> {
    if !request.is_object() {
        return Err(PrivacyError::InvalidRequest(
            "bid request is not a JSON object".into(),
        ));
    }
    let map = request.as_object_mut().unwrap();
    let entry = map.entry(key.to_string()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        return Err(PrivacyError::InvalidRequest(format!(
            "{key} is not an object"
        )));
    }
    Ok(entry)
}

/// Writes TCF/GDPR consent into `regs.ext.gdpr=1` and
/// `user.ext.consent=<consent>`.
#[derive(Debug, Default, Clone)]
pub struct TcfConsentWriter;

impl ConsentWriter for TcfConsentWriter {
    fn write(&self, request: &mut Value, consent: &str) -> Result<(), PrivacyError> {
        {
            let regs = ensure_object(request, "regs")?;
            let ext = ensure_object(regs, "ext")?;
            ext.as_object_mut().unwrap().insert("gdpr".into(), json!(1));
        }
        {
            let user = ensure_object(request, "user")?;
            let ext = ensure_object(user, "ext")?;
            ext.as_object_mut()
                .unwrap()
                .insert("consent".into(), json!(consent));
        }
        Ok(())
    }
}

/// Writes a US-Privacy 1.0 string into `regs.ext.us_privacy`.
#[derive(Debug, Default, Clone)]
pub struct CcpaConsentWriter;

impl ConsentWriter for CcpaConsentWriter {
    fn write(&self, request: &mut Value, consent: &str) -> Result<(), PrivacyError> {
        let regs = ensure_object(request, "regs")?;
        let ext = ensure_object(regs, "ext")?;
        ext.as_object_mut()
            .unwrap()
            .insert("us_privacy".into(), json!(consent));
        Ok(())
    }
}

/// Writes a GPP string into `regs.gpp` along with a list of applying
/// section ids in `regs.gpp_sid`. The `consent` argument is the GPP
/// string; gpp_sid is configured via the writer itself.
#[derive(Debug, Default, Clone)]
pub struct GppConsentWriter {
    pub gpp_sid: Vec<i32>,
}

impl GppConsentWriter {
    pub fn new(gpp_sid: Vec<i32>) -> Self {
        Self { gpp_sid }
    }
}

impl ConsentWriter for GppConsentWriter {
    fn write(&self, request: &mut Value, consent: &str) -> Result<(), PrivacyError> {
        let regs = ensure_object(request, "regs")?;
        let obj = regs.as_object_mut().unwrap();
        obj.insert("gpp".into(), json!(consent));
        obj.insert("gpp_sid".into(), json!(self.gpp_sid));
        Ok(())
    }
}

/// Runs a sequence of consent writers in order; the first failure
/// short-circuits.
pub struct ChainedConsentWriter(pub Vec<Box<dyn ConsentWriter + Send + Sync>>);

impl ChainedConsentWriter {
    pub fn new(writers: Vec<Box<dyn ConsentWriter + Send + Sync>>) -> Self {
        Self(writers)
    }
}

impl ConsentWriter for ChainedConsentWriter {
    fn write(&self, request: &mut Value, consent: &str) -> Result<(), PrivacyError> {
        for w in &self.0 {
            w.write(request, consent)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tcf_writer_sets_fields() {
        let mut req = json!({});
        TcfConsentWriter.write(&mut req, "CONSENT").unwrap();
        assert_eq!(req["regs"]["ext"]["gdpr"], json!(1));
        assert_eq!(req["user"]["ext"]["consent"], json!("CONSENT"));
    }

    #[test]
    fn ccpa_writer_sets_us_privacy() {
        let mut req = json!({});
        CcpaConsentWriter.write(&mut req, "1YNN").unwrap();
        assert_eq!(req["regs"]["ext"]["us_privacy"], json!("1YNN"));
    }

    #[test]
    fn gpp_writer_sets_gpp_fields() {
        let mut req = json!({});
        GppConsentWriter::new(vec![6, 7])
            .write(&mut req, "DBABMA~1YNN")
            .unwrap();
        assert_eq!(req["regs"]["gpp"], json!("DBABMA~1YNN"));
        assert_eq!(req["regs"]["gpp_sid"], json!([6, 7]));
    }

    #[test]
    fn chained_runs_in_order() {
        let mut req = json!({});
        let chain = ChainedConsentWriter::new(vec![
            Box::new(CcpaConsentWriter),
            Box::new(GppConsentWriter::new(vec![6])),
        ]);
        chain.write(&mut req, "1YNN").unwrap();
        assert_eq!(req["regs"]["ext"]["us_privacy"], json!("1YNN"));
        assert_eq!(req["regs"]["gpp"], json!("1YNN"));
        assert_eq!(req["regs"]["gpp_sid"], json!([6]));
    }

    #[test]
    fn rejects_non_object_request() {
        let mut req = json!([]);
        assert!(TcfConsentWriter.write(&mut req, "X").is_err());
    }
}
