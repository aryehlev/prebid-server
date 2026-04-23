//! Environment variable configuration loading.
//!
//! This module provides a higher-level configuration loading pipeline that
//! mirrors the Go Viper pattern used in prebid-server:
//!
//! 1. Load defaults
//! 2. Read from a YAML configuration file (typically `pbs.yaml`)
//! 3. Override with environment variables prefixed with `PBS_`
//!
//! The environment variable mapping converts dots and nested field names to
//! underscores. For example:
//!
//! - `auction_timeouts.default` -> `PBS_AUCTION_TIMEOUTS_DEFAULT`
//! - `gdpr.enabled` -> `PBS_GDPR_ENABLED`
//! - `adapters.appnexus.endpoint` -> `PBS_ADAPTERS_APPNEXUS_ENDPOINT`

use std::path::Path;

use anyhow::Result;

use crate::Configuration;

/// Load configuration using the standard PBS loading pipeline.
///
/// This mirrors Go's Viper-based configuration loading:
/// 1. Start with compiled-in defaults (from `Configuration::default()`)
/// 2. If `yaml_path` is provided and the file exists, overlay the YAML values
/// 3. Overlay environment variables with the `PBS_` prefix
///
/// The `PBS_` prefix is stripped and underscores serve as separators for nested
/// keys. For example, `PBS_AUCTION_TIMEOUTS_DEFAULT=2000` sets
/// `auction_timeouts.default` to `2000`.
///
/// This function delegates to [`Configuration::load`] which already implements
/// the config-rs builder pipeline with file + env sources, and then applies the
/// well-known explicit overrides via [`Configuration::apply_env_overrides`].
pub fn load_config(yaml_path: Option<&str>) -> Result<Configuration> {
    Configuration::load(yaml_path)
}

/// Load configuration from a named YAML file, with environment overrides.
///
/// Convenience wrapper around [`load_config`] for callers that always have a
/// file path. The file must exist and be valid YAML; an error is returned
/// otherwise.
pub fn load_config_from_file(path: &str) -> Result<Configuration> {
    if !Path::new(path).exists() {
        anyhow::bail!("configuration file '{}' does not exist", path);
    }
    load_config(Some(path))
}

/// Load configuration from `pbs.yaml` in the current directory (the typical
/// convention), falling back to defaults + env if the file does not exist.
pub fn load_default_config() -> Result<Configuration> {
    let default_path = "pbs.yaml";
    if Path::new(default_path).exists() {
        load_config(Some(default_path))
    } else {
        load_config(None)
    }
}

/// Resolve a single configuration value from the environment.
///
/// Given a dotted configuration key (e.g. `"auction_timeouts.default"`), this
/// returns the value of the corresponding `PBS_`-prefixed environment variable
/// if it is set.
///
/// The mapping rule is:
///   - Dots (`.`) become underscores (`_`)
///   - The entire key is uppercased
///   - The `PBS_` prefix is prepended
///
/// Example: `"auction_timeouts.default"` -> `PBS_AUCTION_TIMEOUTS_DEFAULT`
pub fn env_value_for_key(key: &str) -> Option<String> {
    let env_key = format!("PBS_{}", key.replace('.', "_").to_uppercase());
    std::env::var(&env_key).ok()
}

/// Convert a dotted configuration key to the corresponding PBS environment
/// variable name.
///
/// Example: `"cache.host"` -> `"PBS_CACHE_HOST"`
pub fn config_key_to_env_var(key: &str) -> String {
    format!("PBS_{}", key.replace('.', "_").to_uppercase())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_key_to_env_var() {
        assert_eq!(
            config_key_to_env_var("auction_timeouts.default"),
            "PBS_AUCTION_TIMEOUTS_DEFAULT"
        );
        assert_eq!(config_key_to_env_var("host"), "PBS_HOST");
        assert_eq!(config_key_to_env_var("gdpr.enabled"), "PBS_GDPR_ENABLED");
        assert_eq!(
            config_key_to_env_var("adapters.appnexus.endpoint"),
            "PBS_ADAPTERS_APPNEXUS_ENDPOINT"
        );
        assert_eq!(
            config_key_to_env_var("cache.default_ttl_secs.banner_ttl_secs"),
            "PBS_CACHE_DEFAULT_TTL_SECS_BANNER_TTL_SECS"
        );
    }

    #[test]
    fn test_env_value_for_key_not_set() {
        // Ensure a key that is very unlikely to be set returns None.
        let val = env_value_for_key("test_very_unlikely_config_key_12345");
        assert!(val.is_none());
    }

    #[test]
    fn test_env_value_for_key_set() {
        let key = "test_env_config_probe";
        let env_name = config_key_to_env_var(key);
        std::env::set_var(&env_name, "hello");
        let val = env_value_for_key(key);
        assert_eq!(val.as_deref(), Some("hello"));
        // Clean up.
        std::env::remove_var(&env_name);
    }

    #[test]
    fn test_load_config_no_file() {
        // Loading with no file and no special env vars should produce a default
        // configuration that passes basic sanity checks.
        let cfg = load_config(None).expect("default config should load");
        assert_eq!(cfg.host, "0.0.0.0");
        assert_eq!(cfg.port, 8000);
    }

    #[test]
    fn test_load_config_from_file_missing() {
        let result = load_config_from_file("/tmp/nonexistent_pbs_test_file_98765.yaml");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_default_config() {
        // In the test working directory there is no pbs.yaml, so this should
        // fall back to defaults.
        let cfg = load_default_config().expect("default config fallback should work");
        assert_eq!(cfg.host, "0.0.0.0");
    }
}
