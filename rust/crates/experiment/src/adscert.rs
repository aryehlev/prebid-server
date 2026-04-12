//! Ads Cert signer configuration.
//!
//! Mirrors the relevant portions of `config/experiment.go` and
//! `experiment/adscert/signer.go` from the Go codebase. Only configuration
//! and mode selection are ported; the actual signing transport is left for
//! a future dedicated crate.

use serde::{Deserialize, Serialize};

/// HTTP header used to carry the ads-cert authentication signature.
pub const SIGN_HEADER: &str = "X-Ads-Cert-Auth";

/// Supported signer modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdCertsSignerMode {
    /// Signing disabled.
    Off,
    /// In-process signer.
    Inprocess,
    /// Remote gRPC signer.
    Remote,
}

impl Default for AdCertsSignerMode {
    fn default() -> Self {
        AdCertsSignerMode::Off
    }
}

impl AdCertsSignerMode {
    /// String representation matching the Go constants.
    pub fn as_str(self) -> &'static str {
        match self {
            AdCertsSignerMode::Off => "off",
            AdCertsSignerMode::Inprocess => "inprocess",
            AdCertsSignerMode::Remote => "remote",
        }
    }
}

/// In-process ads-cert signer configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdsCertInProcess {
    /// ads.cert hostname for the originating party.
    pub origin: String,
    /// Base-64 encoded private key.
    pub private_key: String,
    /// How often to check the `_delivery._adscert` / `_adscert` subdomains.
    pub dns_check_interval_seconds: i64,
    /// How often to renew the `_delivery._adscert` / `_adscert` subdomains.
    pub dns_renewal_interval_seconds: i64,
}

/// Remote ads-cert signer configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdsCertRemote {
    /// Address of the gRPC server that will create a call signature.
    pub url: String,
    /// Maximum time to wait on the remote signer before aborting.
    pub signing_timeout_ms: i64,
}

/// Top-level ads-cert experiment configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExperimentAdsCert {
    /// The signer mode.
    pub mode: AdCertsSignerMode,
    /// In-process signer settings (used when `mode == Inprocess`).
    #[serde(default)]
    pub in_process: AdsCertInProcess,
    /// Remote signer settings (used when `mode == Remote`).
    #[serde(default)]
    pub remote: AdsCertRemote,
}
