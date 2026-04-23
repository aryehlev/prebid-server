//! Multi-source configuration loader.
//!
//! Precedence, lowest to highest:
//!   1. Built-in defaults from [`crate::defaults::default_configuration`]
//!   2. Optional YAML file on disk
//!   3. Environment variables prefixed with `PBS_`
//!
//! The merge is performed with the `config` crate. A final `deserialize()`
//! converts the merged tree into a [`Configuration`].
//!
//! Env vars follow the `config` crate convention: nested keys use `__` as
//! a separator, e.g. `PBS_GDPR__DEFAULT_VALUE=1` sets `gdpr.default_value`.
//! Plain `PBS_PORT=9000` sets the top-level `port` field.

use std::path::Path;

use ::config::{Config, Environment, File, FileFormat};
use serde::Serialize;
use thiserror::Error;

use crate::defaults::default_configuration;
use crate::top::Configuration;

/// Errors emitted by [`ConfigLoader::load`].
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to serialize defaults: {0}")]
    Defaults(#[from] serde_json::Error),
    #[error("failed to build config tree: {0}")]
    Build(#[from] ::config::ConfigError),
    #[error("i/o error reading `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// High-level wrapper around the `config` crate.
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load a [`Configuration`] by merging defaults, an optional YAML file,
    /// and `PBS_*` environment variables.
    pub fn load(path: Option<&Path>) -> Result<Configuration, ConfigError> {
        // 1. Serialize defaults to JSON so `config` can adopt them as a layer.
        let defaults = default_configuration();
        let defaults_json = serde_json::to_string(&defaults)?;

        let mut builder =
            Config::builder().add_source(File::from_str(&defaults_json, FileFormat::Json));

        // 2. Optional YAML file.
        if let Some(p) = path {
            if !p.exists() {
                return Err(ConfigError::Io {
                    path: p.display().to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "config file not found",
                    ),
                });
            }
            builder = builder.add_source(File::from(p).required(true));
        }

        // 3. Env overrides: PBS_PORT, PBS_GDPR__DEFAULT_VALUE, ...
        //
        // We explicitly set `prefix_separator` to a single underscore so
        // that `PBS_PORT` strips to `PORT`, while keeping `separator = "__"`
        // so nested keys like `PBS_GDPR__DEFAULT_VALUE` resolve to
        // `gdpr.default_value`.
        builder = builder.add_source(
            Environment::with_prefix("PBS")
                .prefix_separator("_")
                .separator("__")
                .try_parsing(true),
        );

        let merged = builder.build()?;
        let cfg: Configuration = merged.try_deserialize()?;
        Ok(cfg)
    }
}

/// Load from a YAML string directly (convenience for tests and in-memory use).
pub fn load_from_yaml_str(yaml: &str) -> Result<Configuration, ConfigError> {
    let defaults = default_configuration();
    let defaults_json = serde_json::to_string(&defaults)?;
    let merged = Config::builder()
        .add_source(File::from_str(&defaults_json, FileFormat::Json))
        .add_source(File::from_str(yaml, FileFormat::Yaml))
        .build()?;
    Ok(merged.try_deserialize()?)
}

/// Explicit marker implementation — `Configuration` must be `Serialize` for
/// the defaults layer to work. This `fn` is a compile-time witness.
#[allow(dead_code)]
fn _assert_serialize<T: Serialize>() {}
#[allow(dead_code)]
fn _witness() {
    _assert_serialize::<Configuration>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_from_yaml_string() {
        let yaml = r#"
host: "0.0.0.0"
port: 8123
gdpr:
  default_value: "1"
"#;
        let cfg = load_from_yaml_str(yaml).expect("yaml parse");
        assert_eq!(cfg.host, "0.0.0.0");
        assert_eq!(cfg.port, 8123);
        assert_eq!(cfg.gdpr.default_value, "1");
        // Defaults still shine through:
        assert_eq!(cfg.admin_port, 6060);
    }

    #[test]
    fn defaults_pass_validation() {
        let cfg = default_configuration();
        let errs = cfg.validate();
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    #[test]
    fn invalid_port_produces_error() {
        let mut cfg = default_configuration();
        cfg.port = 0;
        let errs = cfg.validate();
        assert!(
            errs.iter().any(|e| e.field == "port"),
            "expected port error, got {errs:?}"
        );
    }

    #[test]
    fn env_var_override_pbs_port() {
        // SAFETY: single-threaded test — `cargo test` inside this crate
        // executes tests on a thread pool but these env vars are scoped
        // with unique names and we clean them up immediately.
        std::env::set_var("PBS_PORT", "9000");
        let cfg = ConfigLoader::load(None).expect("load");
        std::env::remove_var("PBS_PORT");
        assert_eq!(cfg.port, 9000);
    }

    #[test]
    fn unknown_stored_requests_backend_flags_error() {
        let mut cfg = default_configuration();
        cfg.stored_requests.backend.r#type = "mystery-db".to_string();
        let errs = cfg.validate();
        assert!(
            errs.iter()
                .any(|e| e.field == "stored_requests.backend.type"),
            "expected stored_requests.backend.type error, got {errs:?}"
        );
    }

    #[test]
    fn host_cookie_family_required_when_enabled() {
        let mut cfg = default_configuration();
        cfg.host_cookie.enabled = true;
        cfg.host_cookie.family = String::new();
        let errs = cfg.validate();
        assert!(
            errs.iter().any(|e| e.field == "host_cookie.family"),
            "expected host_cookie.family error, got {errs:?}"
        );
    }
}
