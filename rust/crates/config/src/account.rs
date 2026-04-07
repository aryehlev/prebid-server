//! Account configuration logic ported from Go `config/account.go`.
//!
//! This module adds methods to the existing account config structs defined in
//! `lib.rs`: channel-type lookups, GDPR enforcement helpers, price floor
//! validation, and related utilities.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    AccountCcpaConfig, AccountFloorFetchConfig, AccountGdprConfig,
    AccountGdprPurposeConfig, AccountPriceFloorsConfig, ChannelEnabledConfig,
    ValidationError,
};

// ---------------------------------------------------------------------------
// ChannelType
// ---------------------------------------------------------------------------

/// Channel types that Prebid Server can configure for an account.
///
/// Maps to the Go `ChannelType` constants in config/account.go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelType {
    Amp,
    App,
    Video,
    Web,
    Dooh,
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelType::Amp => write!(f, "amp"),
            ChannelType::App => write!(f, "app"),
            ChannelType::Video => write!(f, "video"),
            ChannelType::Web => write!(f, "web"),
            ChannelType::Dooh => write!(f, "dooh"),
        }
    }
}

// ---------------------------------------------------------------------------
// TCF2 Enforcement Algorithm
// ---------------------------------------------------------------------------

/// TCF2 enforcement algorithm identifier.
///
/// Maps to the Go `TCF2EnforcementAlgo` type in config/config.go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tcf2EnforcementAlgo {
    Undefined,
    Basic,
    Full,
}

impl Tcf2EnforcementAlgo {
    /// Parse from a string value (case-insensitive).
    pub fn from_str_opt(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "basic" => Tcf2EnforcementAlgo::Basic,
            "full" => Tcf2EnforcementAlgo::Full,
            _ => Tcf2EnforcementAlgo::Undefined,
        }
    }
}

// ---------------------------------------------------------------------------
// ChannelEnabledConfig methods
// ---------------------------------------------------------------------------

impl ChannelEnabledConfig {
    /// Look up the enabled flag for a specific channel type.
    ///
    /// Maps to Go `AccountChannel.GetByChannelType()`.
    pub fn get_by_channel_type(&self, channel: ChannelType) -> Option<bool> {
        match channel {
            ChannelType::Amp => self.amp,
            ChannelType::App => self.app,
            ChannelType::Video => self.video,
            ChannelType::Web => self.web,
            ChannelType::Dooh => self.dooh,
        }
    }
}

// ---------------------------------------------------------------------------
// AccountCcpaConfig methods
// ---------------------------------------------------------------------------

impl AccountCcpaConfig {
    /// Whether CCPA is enabled for the given channel type.
    ///
    /// Returns the channel-specific flag if set, otherwise the general
    /// `enabled` flag. `None` means "not configured at account level".
    ///
    /// Maps to Go `AccountCCPA.EnabledForChannelType()`.
    pub fn enabled_for_channel_type(&self, channel: ChannelType) -> Option<bool> {
        if let Some(v) = self.channel_enabled.get_by_channel_type(channel) {
            return Some(v);
        }
        self.enabled
    }
}

// ---------------------------------------------------------------------------
// AccountGdprConfig methods
// ---------------------------------------------------------------------------

impl AccountGdprConfig {
    /// Whether GDPR is enabled for the given channel type.
    ///
    /// Maps to Go `AccountGDPR.EnabledForChannelType()`.
    pub fn enabled_for_channel_type(&self, channel: ChannelType) -> Option<bool> {
        if let Some(v) = self.channel_enabled.get_by_channel_type(channel) {
            return Some(v);
        }
        self.enabled
    }

    /// Get the purpose config for a given purpose number (1-10).
    ///
    /// Returns `None` for out-of-range values.
    pub fn purpose_config(&self, purpose: u8) -> Option<&AccountGdprPurposeConfig> {
        match purpose {
            1 => Some(&self.purpose1),
            2 => Some(&self.purpose2),
            3 => Some(&self.purpose3),
            4 => Some(&self.purpose4),
            5 => Some(&self.purpose5),
            6 => Some(&self.purpose6),
            7 => Some(&self.purpose7),
            8 => Some(&self.purpose8),
            9 => Some(&self.purpose9),
            10 => Some(&self.purpose10),
            _ => None,
        }
    }

    /// Check if purpose enforcement is turned on for a given purpose number.
    ///
    /// Returns `(value, exists)` where `exists` indicates whether the setting
    /// was explicitly configured. If not set, defaults to `true`.
    ///
    /// Maps to Go `AccountGDPR.PurposeEnforced()`.
    pub fn purpose_enforced(&self, purpose: u8) -> (bool, bool) {
        match self.purpose_config(purpose) {
            Some(cfg) => (cfg.enforce_purpose, true),
            None => (true, false),
        }
    }

    /// Check if vendor enforcement is turned on for a given purpose number.
    ///
    /// Returns `(value, exists)`. If not set, defaults to `true`.
    ///
    /// Maps to Go `AccountGDPR.PurposeEnforcingVendors()`.
    pub fn purpose_enforcing_vendors(&self, purpose: u8) -> (bool, bool) {
        match self.purpose_config(purpose) {
            Some(cfg) => (cfg.enforce_vendors, true),
            None => (true, false),
        }
    }

    /// Get the vendor exception set for a given purpose number.
    ///
    /// Returns the set and whether the purpose config exists.
    ///
    /// Maps to Go `AccountGDPR.PurposeVendorExceptions()`.
    pub fn purpose_vendor_exceptions(&self, purpose: u8) -> (HashSet<&str>, bool) {
        match self.purpose_config(purpose) {
            Some(cfg) if !cfg.vendor_exceptions.is_empty() => {
                let set: HashSet<&str> = cfg.vendor_exceptions.iter().map(|s| s.as_str()).collect();
                (set, true)
            }
            Some(_) => (HashSet::new(), true),
            None => (HashSet::new(), false),
        }
    }

    /// Get the enforcement algorithm for a given purpose number.
    ///
    /// Maps to Go `AccountGDPR.PurposeEnforcementAlgo()`.
    pub fn purpose_enforcement_algo(&self, purpose: u8) -> (Tcf2EnforcementAlgo, bool) {
        match self.purpose_config(purpose) {
            Some(cfg) if !cfg.enforce_algo.is_empty() => {
                let algo = Tcf2EnforcementAlgo::from_str_opt(&cfg.enforce_algo);
                if algo == Tcf2EnforcementAlgo::Basic || algo == Tcf2EnforcementAlgo::Full {
                    (algo, true)
                } else {
                    (Tcf2EnforcementAlgo::Undefined, false)
                }
            }
            _ => (Tcf2EnforcementAlgo::Undefined, false),
        }
    }
}

// ---------------------------------------------------------------------------
// AccountPriceFloorsConfig methods
// ---------------------------------------------------------------------------

impl AccountPriceFloorsConfig {
    /// Whether bid adjustment is enabled for price floors.
    ///
    /// Maps to Go `AccountPriceFloors.IsAdjustForBidAdjustmentEnabled()`.
    pub fn is_adjust_for_bid_adjustment_enabled(&self) -> bool {
        self.adjust_for_bid_adjustment
    }

    /// Validate the price floors configuration, returning any errors.
    ///
    /// Maps to Go `AccountPriceFloors.validate()`.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errs = Vec::new();

        if self.enforce_floors_rate > 100 {
            errs.push(verr(
                "account_defaults.price_floors.enforce_floors_rate should be between 0 and 100",
            ));
        }

        if self.max_rules > i32::MAX as u32 {
            errs.push(verr(&format!(
                "account_defaults.price_floors.max_rules should be between 0 and {}",
                i32::MAX
            )));
        }

        if self.max_schema_dims > 20 {
            errs.push(verr(
                "account_defaults.price_floors.max_schema_dims should be between 0 and 20",
            ));
        }

        self.fetch.validate(&mut errs);

        errs
    }
}

impl AccountFloorFetchConfig {
    /// Validate the floor fetch configuration.
    fn validate(&self, errs: &mut Vec<ValidationError>) {
        if self.period_sec > 0 && self.max_age_sec > 0 && self.period_sec > self.max_age_sec {
            errs.push(verr(
                "account_defaults.price_floors.fetch.period_sec should be less than \
                 account_defaults.price_floors.fetch.max_age_sec",
            ));
        }

        if self.period_sec > 0 && self.period_sec < 300 {
            errs.push(verr(
                "account_defaults.price_floors.fetch.period_sec should not be less than 300 seconds",
            ));
        }

        if self.max_age_sec > 0 && self.max_age_sec < 600 {
            errs.push(verr(
                "account_defaults.price_floors.fetch.max_age_sec should not be less than 600 seconds",
            ));
        }

        if self.timeout_ms > 0 && !(self.timeout_ms > 10 && self.timeout_ms < 10000) {
            errs.push(verr(
                "account_defaults.price_floors.fetch.timeout_ms should be between 10 to 10,000 milliseconds",
            ));
        }

        // max_rules and max_file_size_kb are u32 so always >= 0 in Rust
        if self.max_schema_dims > 20 {
            errs.push(verr(
                "account_defaults.price_floors.fetch.max_schema_dims should not be less than 0 and greater than 20",
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// AccountModules helper
// ---------------------------------------------------------------------------

/// Retrieves a module config from a nested vendor->module map.
///
/// The `id` must be in the form "vendor.module_name".
///
/// Maps to Go `AccountModules.ModuleConfig()`.
pub fn module_config<'a>(
    modules: &'a HashMap<String, HashMap<String, serde_json::Value>>,
    id: &str,
) -> Result<Option<&'a serde_json::Value>, String> {
    let parts: Vec<&str> = id.splitn(2, '.').collect();
    if parts.len() < 2 {
        return Err(format!(
            "ID must consist of vendor and module names separated by dot, got: {}",
            id
        ));
    }
    let vendor = parts[0];
    let module = parts[1];
    Ok(modules.get(vendor).and_then(|m| m.get(module)))
}

// ---------------------------------------------------------------------------
// IP validation helpers
// ---------------------------------------------------------------------------

/// Maximum bits in an IPv4 address.
pub const IPV4_BIT_SIZE: u8 = 32;
/// Maximum bits in an IPv6 address.
pub const IPV6_BIT_SIZE: u8 = 128;

/// Validate IPv4 anon_keep_bits.
pub fn validate_ipv4_keep_bits(bits: u8) -> Result<(), String> {
    if bits > IPV4_BIT_SIZE {
        return Err(format!(
            "bits cannot exceed {} in ipv4 address, or be less than 0",
            IPV4_BIT_SIZE
        ));
    }
    Ok(())
}

/// Validate IPv6 anon_keep_bits.
pub fn validate_ipv6_keep_bits(bits: u8) -> Result<(), String> {
    if bits > IPV6_BIT_SIZE {
        return Err(format!(
            "bits cannot exceed {} in ipv6 address, or be less than 0",
            IPV6_BIT_SIZE
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn verr(msg: &str) -> ValidationError {
    ValidationError {
        message: msg.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccountCcpaConfig, AccountGdprConfig, ChannelEnabledConfig};

    // -- ChannelType --

    #[test]
    fn test_channel_type_display() {
        assert_eq!(ChannelType::Amp.to_string(), "amp");
        assert_eq!(ChannelType::App.to_string(), "app");
        assert_eq!(ChannelType::Video.to_string(), "video");
        assert_eq!(ChannelType::Web.to_string(), "web");
        assert_eq!(ChannelType::Dooh.to_string(), "dooh");
    }

    #[test]
    fn test_channel_type_serde_roundtrip() {
        let ct = ChannelType::Web;
        let json = serde_json::to_string(&ct).unwrap();
        assert_eq!(json, "\"web\"");
        let parsed: ChannelType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ChannelType::Web);
    }

    // -- ChannelEnabledConfig --

    #[test]
    fn test_get_by_channel_type_some() {
        let cfg = ChannelEnabledConfig {
            amp: Some(true),
            app: Some(false),
            video: None,
            web: Some(true),
            dooh: Some(false),
        };
        assert_eq!(cfg.get_by_channel_type(ChannelType::Amp), Some(true));
        assert_eq!(cfg.get_by_channel_type(ChannelType::App), Some(false));
        assert_eq!(cfg.get_by_channel_type(ChannelType::Video), None);
        assert_eq!(cfg.get_by_channel_type(ChannelType::Web), Some(true));
        assert_eq!(cfg.get_by_channel_type(ChannelType::Dooh), Some(false));
    }

    #[test]
    fn test_get_by_channel_type_all_none() {
        let cfg = ChannelEnabledConfig::default();
        for ch in &[
            ChannelType::Amp,
            ChannelType::App,
            ChannelType::Video,
            ChannelType::Web,
            ChannelType::Dooh,
        ] {
            assert_eq!(cfg.get_by_channel_type(*ch), None);
        }
    }

    // -- AccountCcpaConfig --

    #[test]
    fn test_ccpa_enabled_for_channel_uses_channel() {
        let ccpa = AccountCcpaConfig {
            enabled: Some(false),
            channel_enabled: ChannelEnabledConfig {
                amp: Some(true),
                ..Default::default()
            },
        };
        // Channel-specific overrides the general setting
        assert_eq!(ccpa.enabled_for_channel_type(ChannelType::Amp), Some(true));
        // Fallback to general
        assert_eq!(ccpa.enabled_for_channel_type(ChannelType::Web), Some(false));
    }

    #[test]
    fn test_ccpa_enabled_for_channel_none() {
        let ccpa = AccountCcpaConfig::default();
        assert_eq!(ccpa.enabled_for_channel_type(ChannelType::App), None);
    }

    // -- AccountGdprConfig --

    #[test]
    fn test_gdpr_enabled_for_channel_uses_channel() {
        let gdpr = AccountGdprConfig {
            enabled: Some(true),
            channel_enabled: ChannelEnabledConfig {
                app: Some(false),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            gdpr.enabled_for_channel_type(ChannelType::App),
            Some(false)
        );
        assert_eq!(gdpr.enabled_for_channel_type(ChannelType::Web), Some(true));
    }

    #[test]
    fn test_gdpr_enabled_for_channel_none() {
        let gdpr = AccountGdprConfig::default();
        assert_eq!(gdpr.enabled_for_channel_type(ChannelType::Amp), None);
    }

    // -- Purpose helpers --

    #[test]
    fn test_purpose_enforced_default() {
        let gdpr = AccountGdprConfig::default();
        // All defaults: enforce_purpose is false (serde default), but
        // purpose_config returns Some, so exists=true and value=false.
        let (val, exists) = gdpr.purpose_enforced(1);
        assert!(!val); // default bool is false
        assert!(exists);
    }

    #[test]
    fn test_purpose_enforced_out_of_range() {
        let gdpr = AccountGdprConfig::default();
        let (val, exists) = gdpr.purpose_enforced(0);
        assert!(val); // default true when not found
        assert!(!exists);

        let (val, exists) = gdpr.purpose_enforced(11);
        assert!(val);
        assert!(!exists);
    }

    #[test]
    fn test_purpose_enforced_explicit() {
        let mut gdpr = AccountGdprConfig::default();
        gdpr.purpose3.enforce_purpose = true;
        let (val, exists) = gdpr.purpose_enforced(3);
        assert!(val);
        assert!(exists);
    }

    #[test]
    fn test_purpose_enforcing_vendors_default() {
        let gdpr = AccountGdprConfig::default();
        let (val, exists) = gdpr.purpose_enforcing_vendors(1);
        assert!(!val); // default bool is false
        assert!(exists);
    }

    #[test]
    fn test_purpose_vendor_exceptions() {
        let mut gdpr = AccountGdprConfig::default();
        gdpr.purpose2.vendor_exceptions = vec!["bidderA".to_string(), "bidderB".to_string()];
        let (set, exists) = gdpr.purpose_vendor_exceptions(2);
        assert!(exists);
        assert!(set.contains("bidderA"));
        assert!(set.contains("bidderB"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_purpose_enforcement_algo() {
        let mut gdpr = AccountGdprConfig::default();
        gdpr.purpose1.enforce_algo = "full".to_string();
        let (algo, exists) = gdpr.purpose_enforcement_algo(1);
        assert_eq!(algo, Tcf2EnforcementAlgo::Full);
        assert!(exists);

        gdpr.purpose2.enforce_algo = "basic".to_string();
        let (algo, exists) = gdpr.purpose_enforcement_algo(2);
        assert_eq!(algo, Tcf2EnforcementAlgo::Basic);
        assert!(exists);

        // Undefined
        let (algo, exists) = gdpr.purpose_enforcement_algo(5);
        assert_eq!(algo, Tcf2EnforcementAlgo::Undefined);
        assert!(!exists);
    }

    // -- Price floors validation --

    #[test]
    fn test_price_floors_valid() {
        let pf = AccountPriceFloorsConfig {
            enabled: true,
            enforce_floors_rate: 50,
            adjust_for_bid_adjustment: true,
            enforce_deal_floors: true,
            use_dynamic_data: false,
            max_rules: 100,
            max_schema_dims: 10,
            fetch: AccountFloorFetchConfig::default(),
        };
        let errs = pf.validate();
        assert!(errs.is_empty(), "expected no errors, got: {:?}", errs);
    }

    #[test]
    fn test_price_floors_invalid_rate() {
        let pf = AccountPriceFloorsConfig {
            enforce_floors_rate: 101,
            ..Default::default()
        };
        let errs = pf.validate();
        assert!(errs.iter().any(|e| e.message.contains("enforce_floors_rate")));
    }

    #[test]
    fn test_price_floors_invalid_schema_dims() {
        let pf = AccountPriceFloorsConfig {
            max_schema_dims: 21,
            ..Default::default()
        };
        let errs = pf.validate();
        assert!(errs.iter().any(|e| e.message.contains("max_schema_dims")));
    }

    #[test]
    fn test_is_adjust_for_bid_adjustment_enabled() {
        let pf = AccountPriceFloorsConfig {
            adjust_for_bid_adjustment: true,
            ..Default::default()
        };
        assert!(pf.is_adjust_for_bid_adjustment_enabled());

        let pf2 = AccountPriceFloorsConfig::default();
        assert!(!pf2.is_adjust_for_bid_adjustment_enabled());
    }

    // -- Floor fetch validation --

    #[test]
    fn test_floor_fetch_period_exceeds_max_age() {
        let pf = AccountPriceFloorsConfig {
            fetch: AccountFloorFetchConfig {
                period_sec: 700,
                max_age_sec: 600,
                ..Default::default()
            },
            ..Default::default()
        };
        let errs = pf.validate();
        assert!(errs
            .iter()
            .any(|e| e.message.contains("period_sec should be less than")));
    }

    #[test]
    fn test_floor_fetch_period_too_small() {
        let pf = AccountPriceFloorsConfig {
            fetch: AccountFloorFetchConfig {
                period_sec: 100,
                max_age_sec: 700,
                ..Default::default()
            },
            ..Default::default()
        };
        let errs = pf.validate();
        assert!(errs
            .iter()
            .any(|e| e.message.contains("period_sec should not be less than 300")));
    }

    #[test]
    fn test_floor_fetch_max_age_too_small() {
        let pf = AccountPriceFloorsConfig {
            fetch: AccountFloorFetchConfig {
                period_sec: 300,
                max_age_sec: 500,
                ..Default::default()
            },
            ..Default::default()
        };
        let errs = pf.validate();
        assert!(errs
            .iter()
            .any(|e| e.message.contains("max_age_sec should not be less than 600")));
    }

    #[test]
    fn test_floor_fetch_timeout_out_of_range() {
        let pf = AccountPriceFloorsConfig {
            fetch: AccountFloorFetchConfig {
                timeout_ms: 5,
                ..Default::default()
            },
            ..Default::default()
        };
        let errs = pf.validate();
        assert!(errs.iter().any(|e| e.message.contains("timeout_ms")));
    }

    // -- Module config --

    #[test]
    fn test_module_config_valid() {
        let mut vendor = HashMap::new();
        vendor.insert("mymodule".to_string(), serde_json::json!({"key": "val"}));
        let mut modules = HashMap::new();
        modules.insert("myvendor".to_string(), vendor);

        let result = module_config(&modules, "myvendor.mymodule");
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(val.is_some());
        assert_eq!(val.unwrap()["key"], "val");
    }

    #[test]
    fn test_module_config_invalid_id() {
        let modules = HashMap::new();
        let result = module_config(&modules, "nodotshere");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("separated by dot"));
    }

    #[test]
    fn test_module_config_not_found() {
        let modules = HashMap::new();
        let result = module_config(&modules, "vendor.module");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // -- IP validation --

    #[test]
    fn test_validate_ipv4_keep_bits() {
        assert!(validate_ipv4_keep_bits(0).is_ok());
        assert!(validate_ipv4_keep_bits(32).is_ok());
        assert!(validate_ipv4_keep_bits(33).is_err());
    }

    #[test]
    fn test_validate_ipv6_keep_bits() {
        assert!(validate_ipv6_keep_bits(0).is_ok());
        assert!(validate_ipv6_keep_bits(128).is_ok());
        assert!(validate_ipv6_keep_bits(129).is_err());
    }

    // -- Tcf2EnforcementAlgo --

    #[test]
    fn test_enforcement_algo_parsing() {
        assert_eq!(
            Tcf2EnforcementAlgo::from_str_opt("basic"),
            Tcf2EnforcementAlgo::Basic
        );
        assert_eq!(
            Tcf2EnforcementAlgo::from_str_opt("FULL"),
            Tcf2EnforcementAlgo::Full
        );
        assert_eq!(
            Tcf2EnforcementAlgo::from_str_opt("unknown"),
            Tcf2EnforcementAlgo::Undefined
        );
        assert_eq!(
            Tcf2EnforcementAlgo::from_str_opt(""),
            Tcf2EnforcementAlgo::Undefined
        );
    }
}
