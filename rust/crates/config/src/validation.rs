//! Semantic validation for the new multi-source [`Configuration`].
//!
//! The Go implementation lives in `config/config.go` (see `(*Configuration).validate`)
//! and `config/validate.go`. This port focuses on the common checks used by
//! the loader at startup.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::top::Configuration;

/// A single semantic validation error, tagged with the offending field path.
#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq, Eq)]
#[error("config validation failed at `{field}`: {message}")]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl Configuration {
    /// Run all semantic validation checks over this [`Configuration`] and
    /// return a (possibly empty) list of [`ValidationError`]s.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errs = Vec::new();

        // --- server ports --------------------------------------------------
        if self.port == 0 {
            errs.push(ValidationError::new(
                "port",
                "port must be greater than 0",
            ));
        }
        if self.admin_port == 0 {
            errs.push(ValidationError::new(
                "admin_port",
                "admin_port must be greater than 0",
            ));
        }

        // --- GDPR ----------------------------------------------------------
        validate_gdpr(self, &mut errs);

        // --- Currency ------------------------------------------------------
        validate_currency(self, &mut errs);

        // --- Stored requests backend --------------------------------------
        validate_stored_requests(self, &mut errs);

        // --- Host cookie ---------------------------------------------------
        validate_host_cookie(self, &mut errs);

        errs
    }
}

fn validate_gdpr(cfg: &Configuration, errs: &mut Vec<ValidationError>) {
    // Must be "0" or "1" (string) per Go semantics.
    let dv = cfg.gdpr.default_value.as_str();
    if dv != "0" && dv != "1" {
        errs.push(ValidationError::new(
            "gdpr.default_value",
            format!("must be \"0\" or \"1\", got \"{dv}\""),
        ));
    }
}

fn validate_currency(cfg: &Configuration, errs: &mut Vec<ValidationError>) {
    let url = cfg.currency.fetch_url.as_str();
    if url.is_empty() {
        // Empty is allowed — currency converter is simply disabled.
        return;
    }
    if !is_parseable_url(url) {
        errs.push(ValidationError::new(
            "currency.fetch_url",
            format!("not a parseable URL: \"{url}\""),
        ));
    }
}

fn validate_stored_requests(cfg: &Configuration, errs: &mut Vec<ValidationError>) {
    const KNOWN: &[&str] = &["none", "file", "postgres", "http", "memory", ""];
    let t = cfg.stored_requests.backend.r#type.as_str();
    if !KNOWN.contains(&t) {
        errs.push(ValidationError::new(
            "stored_requests.backend.type",
            format!(
                "unknown backend type \"{t}\" (expected one of: {})",
                KNOWN.join(", ")
            ),
        ));
    }
}

fn validate_host_cookie(cfg: &Configuration, errs: &mut Vec<ValidationError>) {
    if cfg.host_cookie.enabled && cfg.host_cookie.family.trim().is_empty() {
        errs.push(ValidationError::new(
            "host_cookie.family",
            "must be non-empty when host_cookie.enabled = true",
        ));
    }
}

/// Minimal URL parseability check: requires a non-empty scheme and host.
/// Deliberately avoids pulling in a full URL crate — mirrors what the Go
/// code does with `url.Parse`.
fn is_parseable_url(s: &str) -> bool {
    let Some(scheme_end) = s.find("://") else {
        return false;
    };
    if scheme_end == 0 {
        return false;
    }
    let rest = &s[scheme_end + 3..];
    // Host ends at first '/', '?' or '#'.
    let host_end = rest
        .find(|c: char| c == '/' || c == '?' || c == '#')
        .unwrap_or(rest.len());
    !rest[..host_end].is_empty()
}
