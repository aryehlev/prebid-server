//! Top-level experiment configuration and validation.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::adscert::{AdCertsSignerMode, ExperimentAdsCert};

/// Collected experiment configuration. This maps one-to-one onto the Go
/// `config.Experiment` struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Experiment {
    /// Ads Cert signing settings.
    #[serde(default, rename = "adscert")]
    pub ad_certs: ExperimentAdsCert,
}

/// Validation errors for experiment configuration.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExperimentValidationError {
    /// The in-process signer origin is missing or invalid.
    #[error("invalid url for inprocess signer: {0}")]
    InProcessInvalidUrl(String),
    /// The in-process signer private key is missing.
    #[error("private key for inprocess signer cannot be empty")]
    InProcessInvalidPrivateKey,
    /// The in-process signer DNS renewal interval is invalid.
    #[error("invalid dns renewal interval for inprocess signer: {0}")]
    InProcessInvalidDnsRenewal(i64),
    /// The in-process signer DNS check interval is invalid.
    #[error("invalid dns check interval for inprocess signer: {0}")]
    InProcessInvalidDnsCheck(i64),
    /// The remote signer URL is missing or invalid.
    #[error("invalid url for remote signer: {0}")]
    RemoteInvalidUrl(String),
    /// The remote signer timeout is invalid.
    #[error("invalid signing timeout for remote signer: {0}")]
    RemoteInvalidTimeout(i64),
}

fn looks_like_url(candidate: &str) -> bool {
    // Very small URL-ish validator, intentionally permissive. Mirrors the
    // intent of `url.ParseRequestURI` in Go without pulling in a URL crate.
    if candidate.is_empty() {
        return false;
    }
    let Some((scheme, rest)) = candidate.split_once("://") else {
        return false;
    };
    !scheme.is_empty() && !rest.is_empty() && !scheme.contains(' ')
}

impl Experiment {
    /// Validate the configuration, returning every error found.
    ///
    /// Mirrors the Go `Experiment.validate` method.
    pub fn validate(&self) -> Vec<ExperimentValidationError> {
        let mut errs = Vec::new();
        let ac = &self.ad_certs;
        match ac.mode {
            AdCertsSignerMode::Off => {}
            AdCertsSignerMode::Inprocess => {
                if !looks_like_url(&ac.in_process.origin) {
                    errs.push(ExperimentValidationError::InProcessInvalidUrl(
                        ac.in_process.origin.clone(),
                    ));
                }
                if ac.in_process.private_key.is_empty() {
                    errs.push(ExperimentValidationError::InProcessInvalidPrivateKey);
                }
                if ac.in_process.dns_renewal_interval_seconds <= 0 {
                    errs.push(ExperimentValidationError::InProcessInvalidDnsRenewal(
                        ac.in_process.dns_renewal_interval_seconds,
                    ));
                }
                if ac.in_process.dns_check_interval_seconds <= 0 {
                    errs.push(ExperimentValidationError::InProcessInvalidDnsCheck(
                        ac.in_process.dns_check_interval_seconds,
                    ));
                }
            }
            AdCertsSignerMode::Remote => {
                if !looks_like_url(&ac.remote.url) {
                    errs.push(ExperimentValidationError::RemoteInvalidUrl(
                        ac.remote.url.clone(),
                    ));
                }
                if ac.remote.signing_timeout_ms <= 0 {
                    errs.push(ExperimentValidationError::RemoteInvalidTimeout(
                        ac.remote.signing_timeout_ms,
                    ));
                }
            }
        }
        errs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adscert::{AdsCertInProcess, AdsCertRemote};

    #[test]
    fn off_mode_never_errors() {
        let exp = Experiment::default();
        assert!(exp.validate().is_empty());
    }

    #[test]
    fn valid_inprocess_config() {
        let exp = Experiment {
            ad_certs: ExperimentAdsCert {
                mode: AdCertsSignerMode::Inprocess,
                in_process: AdsCertInProcess {
                    origin: "http://example.com".to_string(),
                    private_key: "pk".to_string(),
                    dns_check_interval_seconds: 10,
                    dns_renewal_interval_seconds: 10,
                },
                ..Default::default()
            },
        };
        assert!(exp.validate().is_empty());
    }

    #[test]
    fn invalid_inprocess_config_reports_all_errors() {
        let exp = Experiment {
            ad_certs: ExperimentAdsCert {
                mode: AdCertsSignerMode::Inprocess,
                in_process: AdsCertInProcess {
                    origin: "not_a_url".to_string(),
                    private_key: "".to_string(),
                    dns_check_interval_seconds: 0,
                    dns_renewal_interval_seconds: 0,
                },
                ..Default::default()
            },
        };
        let errs = exp.validate();
        assert_eq!(errs.len(), 4);
    }

    #[test]
    fn valid_remote_config() {
        let exp = Experiment {
            ad_certs: ExperimentAdsCert {
                mode: AdCertsSignerMode::Remote,
                remote: AdsCertRemote {
                    url: "http://example.com".to_string(),
                    signing_timeout_ms: 5,
                },
                ..Default::default()
            },
        };
        assert!(exp.validate().is_empty());
    }

    #[test]
    fn invalid_remote_config() {
        let exp = Experiment {
            ad_certs: ExperimentAdsCert {
                mode: AdCertsSignerMode::Remote,
                remote: AdsCertRemote {
                    url: "".to_string(),
                    signing_timeout_ms: 0,
                },
                ..Default::default()
            },
        };
        let errs = exp.validate();
        assert_eq!(errs.len(), 2);
    }
}
