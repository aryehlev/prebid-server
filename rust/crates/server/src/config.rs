//! Server configuration loaded from environment variables.
//!
//! Mirrors a subset of the Go [`config.Server`] struct used by the existing
//! prebid-server binary.  Only the host/port plus coarse read/write timeouts
//! are modelled here — this is sufficient for wiring up the HTTP router
//! skeleton.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Default bind host (matches Go `server.host` default).
pub const DEFAULT_HOST: &str = "0.0.0.0";
/// Default bind port (matches Go `server.port` default of 8000).
pub const DEFAULT_PORT: u16 = 8000;
/// Default read timeout in seconds.
pub const DEFAULT_READ_TIMEOUT_SECS: u64 = 15;
/// Default write timeout in seconds.
pub const DEFAULT_WRITE_TIMEOUT_SECS: u64 = 15;

/// Errors that can occur while loading server configuration from the
/// environment.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid value for {var}: {value:?} ({source})")]
    InvalidValue {
        var: &'static str,
        value: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// Top-level server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Bind host (IP or hostname).
    pub host: String,
    /// Bind port.
    pub port: u16,
    /// Per-request read timeout.
    #[serde(with = "duration_secs")]
    pub read_timeout: Duration,
    /// Per-request write timeout.
    #[serde(with = "duration_secs")]
    pub write_timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
            read_timeout: Duration::from_secs(DEFAULT_READ_TIMEOUT_SECS),
            write_timeout: Duration::from_secs(DEFAULT_WRITE_TIMEOUT_SECS),
        }
    }
}

impl ServerConfig {
    /// Load the configuration from the process environment.
    ///
    /// Recognised variables:
    ///
    /// | Variable                | Default |
    /// |-------------------------|---------|
    /// | `PBS_HOST`              | `0.0.0.0` |
    /// | `PBS_PORT` (or `PORT`)  | `8000`    |
    /// | `PBS_READ_TIMEOUT_SECS` | `15`      |
    /// | `PBS_WRITE_TIMEOUT_SECS`| `15`      |
    pub fn from_env() -> Result<Self, ConfigError> {
        let mut cfg = Self::default();

        if let Ok(host) = std::env::var("PBS_HOST") {
            if !host.is_empty() {
                cfg.host = host;
            }
        }

        // PBS_PORT wins over PORT, which matches the prevailing convention in
        // the rest of the Rust port.
        let port_var = std::env::var("PBS_PORT")
            .ok()
            .or_else(|| std::env::var("PORT").ok());
        if let Some(port) = port_var {
            cfg.port = port.parse().map_err(|e: std::num::ParseIntError| {
                ConfigError::InvalidValue {
                    var: "PBS_PORT",
                    value: port,
                    source: Box::new(e),
                }
            })?;
        }

        if let Ok(secs) = std::env::var("PBS_READ_TIMEOUT_SECS") {
            let parsed: u64 = secs.parse().map_err(|e: std::num::ParseIntError| {
                ConfigError::InvalidValue {
                    var: "PBS_READ_TIMEOUT_SECS",
                    value: secs,
                    source: Box::new(e),
                }
            })?;
            cfg.read_timeout = Duration::from_secs(parsed);
        }

        if let Ok(secs) = std::env::var("PBS_WRITE_TIMEOUT_SECS") {
            let parsed: u64 = secs.parse().map_err(|e: std::num::ParseIntError| {
                ConfigError::InvalidValue {
                    var: "PBS_WRITE_TIMEOUT_SECS",
                    value: secs,
                    source: Box::new(e),
                }
            })?;
            cfg.write_timeout = Duration::from_secs(parsed);
        }

        Ok(cfg)
    }

    /// Resolve the configured host+port into a [`SocketAddr`].
    ///
    /// If the host string does not parse as an IP literal, fall back to the
    /// unspecified address (`0.0.0.0`).
    pub fn socket_addr(&self) -> SocketAddr {
        let ip: IpAddr = self
            .host
            .parse()
            .unwrap_or_else(|_| IpAddr::from([0, 0, 0, 0]));
        SocketAddr::new(ip, self.port)
    }
}

mod duration_secs {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(d.as_secs())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let secs = u64::deserialize(d)?;
        Ok(Duration::from_secs(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.host, DEFAULT_HOST);
        assert_eq!(cfg.port, DEFAULT_PORT);
        assert_eq!(cfg.read_timeout, Duration::from_secs(DEFAULT_READ_TIMEOUT_SECS));
        assert_eq!(cfg.write_timeout, Duration::from_secs(DEFAULT_WRITE_TIMEOUT_SECS));
    }

    #[test]
    fn socket_addr_parses_ip() {
        let cfg = ServerConfig {
            host: "127.0.0.1".into(),
            port: 9999,
            ..Default::default()
        };
        assert_eq!(cfg.socket_addr().to_string(), "127.0.0.1:9999");
    }

    #[test]
    fn socket_addr_falls_back_for_hostname() {
        let cfg = ServerConfig {
            host: "not-an-ip".into(),
            port: 1234,
            ..Default::default()
        };
        assert_eq!(cfg.socket_addr().to_string(), "0.0.0.0:1234");
    }
}
