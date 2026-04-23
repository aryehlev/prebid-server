//! GDPR/TCF2 aggregated configuration.
//!
//! Mirrors Go `gdpr/aggregated_config.go`.
//!
//! Provides a unified view of TCF2 enforcement configuration, merging
//! host-level and account-level settings with appropriate precedence.

use std::collections::HashSet;

// ---------------------------------------------------------------------------
// TCF2Config — aggregated TCF2 configuration
// ---------------------------------------------------------------------------

/// Aggregated TCF2 configuration that merges host and account settings.
///
/// Account-level settings override host-level when present.
///
/// Mirrors Go `gdpr.tcf2Config`.
#[derive(Debug, Clone)]
pub struct Tcf2Config {
    /// Host-level TCF2 configuration.
    pub host: HostTcf2Config,
    /// Account-level overrides (optional fields).
    pub account: AccountGdprConfig,
}

/// Host-level TCF2 enforcement configuration.
///
/// Mirrors Go `config.TCF2`.
#[derive(Debug, Clone, Default)]
pub struct HostTcf2Config {
    pub enabled: bool,
    /// Feature 1: precise geolocation data.
    pub special_feature1_enforce: bool,
    pub special_feature1_vendor_exceptions: HashSet<String>,
    /// Purpose 1 treatment (EU jurisdiction).
    pub purpose_one_treatment_enabled: bool,
    pub purpose_one_treatment_access_allowed: bool,
    /// Per-purpose enforcement (1-10).
    pub purposes: [PurposeEnforcementConfig; 10],
    /// Channel-specific enablement.
    pub channel_enabled: ChannelEnabledConfig,
}

/// Per-purpose enforcement configuration.
#[derive(Debug, Clone, Default)]
pub struct PurposeEnforcementConfig {
    /// Whether to enforce this purpose's consent.
    pub enforce_purpose: bool,
    /// Whether to enforce vendor consent for this purpose.
    pub enforce_vendors: bool,
    /// Vendor exceptions for this purpose.
    pub vendor_exceptions: HashSet<String>,
    /// Enforcement algorithm: "basic" or "full".
    pub enforce_algo: String,
}

/// Channel-specific GDPR enablement.
#[derive(Debug, Clone, Default)]
pub struct ChannelEnabledConfig {
    pub web: bool,
    pub amp: bool,
    pub app: bool,
    pub video: bool,
    pub dooh: bool,
}

/// Account-level GDPR configuration overrides.
///
/// Fields are Option to distinguish "not set" from explicit values.
///
/// Mirrors Go `config.AccountGDPR`.
#[derive(Debug, Clone, Default)]
pub struct AccountGdprConfig {
    pub enabled: Option<bool>,
    /// Per-purpose overrides (indexed 0-9 for purposes 1-10).
    pub purpose_overrides: [AccountPurposeOverride; 10],
    pub special_feature1_vendor_exceptions: Option<HashSet<String>>,
    pub basic_enforcement_vendors: Option<HashSet<String>>,
}

/// Account-level per-purpose enforcement overrides.
#[derive(Debug, Clone, Default)]
pub struct AccountPurposeOverride {
    pub enforce_purpose: Option<bool>,
    pub enforce_vendors: Option<bool>,
    pub vendor_exceptions: Option<HashSet<String>>,
}

// ---------------------------------------------------------------------------
// TCF2ConfigReader trait
// ---------------------------------------------------------------------------

/// Trait for reading aggregated TCF2 configuration.
///
/// Mirrors Go `gdpr.TCF2ConfigReader`.
pub trait Tcf2ConfigReader: Send + Sync {
    /// Whether GDPR enforcement is enabled.
    fn is_enabled(&self) -> bool;

    /// Whether a specific channel has GDPR enabled.
    fn channel_enabled(&self, channel: ChannelType) -> bool;

    /// Whether a specific purpose is enforced.
    fn purpose_enforced(&self, purpose: u32) -> bool;

    /// The enforcement algorithm for a purpose.
    fn purpose_enforcement_algo(&self, purpose: u32) -> String;

    /// Whether vendor consent is enforced for a purpose.
    fn purpose_enforcing_vendors(&self, purpose: u32) -> bool;

    /// Vendor exceptions for a purpose.
    fn purpose_vendor_exceptions(&self, purpose: u32) -> HashSet<String>;

    /// Whether special feature 1 (geo) is enforced.
    fn feature_one_enforced(&self) -> bool;

    /// Whether a vendor is exempt from special feature 1.
    fn feature_one_vendor_exception(&self, bidder: &str) -> bool;

    /// Whether purpose one treatment is enabled.
    fn purpose_one_treatment_enabled(&self) -> bool;

    /// Whether access is allowed under purpose one treatment.
    fn purpose_one_treatment_access_allowed(&self) -> bool;

    /// Vendors using basic enforcement algorithm.
    fn basic_enforcement_vendors(&self) -> HashSet<String>;
}

/// Channel types for GDPR enforcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelType {
    Web,
    Amp,
    App,
    Video,
    Dooh,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl Tcf2ConfigReader for Tcf2Config {
    fn is_enabled(&self) -> bool {
        self.account.enabled.unwrap_or(self.host.enabled)
    }

    fn channel_enabled(&self, channel: ChannelType) -> bool {
        if !self.is_enabled() {
            return false;
        }
        match channel {
            ChannelType::Web => self.host.channel_enabled.web,
            ChannelType::Amp => self.host.channel_enabled.amp,
            ChannelType::App => self.host.channel_enabled.app,
            ChannelType::Video => self.host.channel_enabled.video,
            ChannelType::Dooh => self.host.channel_enabled.dooh,
        }
    }

    fn purpose_enforced(&self, purpose: u32) -> bool {
        if purpose == 0 || purpose > 10 {
            return false;
        }
        let idx = (purpose - 1) as usize;
        self.account.purpose_overrides[idx]
            .enforce_purpose
            .unwrap_or(self.host.purposes[idx].enforce_purpose)
    }

    fn purpose_enforcement_algo(&self, purpose: u32) -> String {
        if purpose == 0 || purpose > 10 {
            return "basic".to_string();
        }
        let idx = (purpose - 1) as usize;
        self.host.purposes[idx].enforce_algo.clone()
    }

    fn purpose_enforcing_vendors(&self, purpose: u32) -> bool {
        if purpose == 0 || purpose > 10 {
            return false;
        }
        let idx = (purpose - 1) as usize;
        self.account.purpose_overrides[idx]
            .enforce_vendors
            .unwrap_or(self.host.purposes[idx].enforce_vendors)
    }

    fn purpose_vendor_exceptions(&self, purpose: u32) -> HashSet<String> {
        if purpose == 0 || purpose > 10 {
            return HashSet::new();
        }
        let idx = (purpose - 1) as usize;
        self.account.purpose_overrides[idx]
            .vendor_exceptions
            .clone()
            .unwrap_or_else(|| self.host.purposes[idx].vendor_exceptions.clone())
    }

    fn feature_one_enforced(&self) -> bool {
        self.host.special_feature1_enforce
    }

    fn feature_one_vendor_exception(&self, bidder: &str) -> bool {
        let exceptions = self
            .account
            .special_feature1_vendor_exceptions
            .as_ref()
            .unwrap_or(&self.host.special_feature1_vendor_exceptions);
        exceptions.contains(bidder)
    }

    fn purpose_one_treatment_enabled(&self) -> bool {
        self.host.purpose_one_treatment_enabled
    }

    fn purpose_one_treatment_access_allowed(&self) -> bool {
        self.host.purpose_one_treatment_access_allowed
    }

    fn basic_enforcement_vendors(&self) -> HashSet<String> {
        self.account
            .basic_enforcement_vendors
            .clone()
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> Tcf2Config {
        let mut host = HostTcf2Config {
            enabled: true,
            special_feature1_enforce: true,
            purpose_one_treatment_enabled: false,
            purpose_one_treatment_access_allowed: false,
            channel_enabled: ChannelEnabledConfig {
                web: true,
                amp: true,
                app: true,
                video: true,
                dooh: false,
            },
            ..Default::default()
        };

        // Set purpose 1 and 2 enforcement
        host.purposes[0] = PurposeEnforcementConfig {
            enforce_purpose: true,
            enforce_vendors: true,
            enforce_algo: "basic".to_string(),
            ..Default::default()
        };
        host.purposes[1] = PurposeEnforcementConfig {
            enforce_purpose: true,
            enforce_vendors: true,
            enforce_algo: "full".to_string(),
            ..Default::default()
        };

        Tcf2Config {
            host,
            account: AccountGdprConfig::default(),
        }
    }

    #[test]
    fn test_is_enabled_default() {
        let config = default_config();
        assert!(config.is_enabled());
    }

    #[test]
    fn test_is_enabled_account_override() {
        let mut config = default_config();
        config.account.enabled = Some(false);
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_channel_enabled() {
        let config = default_config();
        assert!(config.channel_enabled(ChannelType::Web));
        assert!(config.channel_enabled(ChannelType::App));
        assert!(!config.channel_enabled(ChannelType::Dooh));
    }

    #[test]
    fn test_channel_disabled_when_gdpr_off() {
        let mut config = default_config();
        config.host.enabled = false;
        assert!(!config.channel_enabled(ChannelType::Web));
    }

    #[test]
    fn test_purpose_enforced() {
        let config = default_config();
        assert!(config.purpose_enforced(1));
        assert!(config.purpose_enforced(2));
        assert!(!config.purpose_enforced(3)); // not configured
    }

    #[test]
    fn test_purpose_enforced_account_override() {
        let mut config = default_config();
        config.account.purpose_overrides[0].enforce_purpose = Some(false);
        assert!(!config.purpose_enforced(1));
    }

    #[test]
    fn test_purpose_enforcement_algo() {
        let config = default_config();
        assert_eq!(config.purpose_enforcement_algo(1), "basic");
        assert_eq!(config.purpose_enforcement_algo(2), "full");
    }

    #[test]
    fn test_purpose_vendor_exceptions() {
        let mut config = default_config();
        let mut exceptions = HashSet::new();
        exceptions.insert("trusted_bidder".to_string());
        config.host.purposes[0].vendor_exceptions = exceptions;

        let result = config.purpose_vendor_exceptions(1);
        assert!(result.contains("trusted_bidder"));
    }

    #[test]
    fn test_purpose_vendor_exceptions_account_override() {
        let mut config = default_config();
        let mut host_exceptions = HashSet::new();
        host_exceptions.insert("host_vendor".to_string());
        config.host.purposes[0].vendor_exceptions = host_exceptions;

        let mut account_exceptions = HashSet::new();
        account_exceptions.insert("account_vendor".to_string());
        config.account.purpose_overrides[0].vendor_exceptions = Some(account_exceptions);

        let result = config.purpose_vendor_exceptions(1);
        assert!(!result.contains("host_vendor"));
        assert!(result.contains("account_vendor"));
    }

    #[test]
    fn test_feature_one_enforced() {
        let config = default_config();
        assert!(config.feature_one_enforced());
    }

    #[test]
    fn test_feature_one_vendor_exception() {
        let mut config = default_config();
        config
            .host
            .special_feature1_vendor_exceptions
            .insert("exempt_bidder".to_string());

        assert!(config.feature_one_vendor_exception("exempt_bidder"));
        assert!(!config.feature_one_vendor_exception("other_bidder"));
    }

    #[test]
    fn test_purpose_out_of_range() {
        let config = default_config();
        assert!(!config.purpose_enforced(0));
        assert!(!config.purpose_enforced(11));
        assert!(config.purpose_vendor_exceptions(0).is_empty());
    }
}
