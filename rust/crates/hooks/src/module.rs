//! Module registration.
//!
//! A *module* groups together a collection of hooks that share a common
//! name, configuration and lifecycle. In the Go codebase this is represented
//! by the `HookRepository` / module builder functions under
//! `hooks/repo.go`; this crate exposes a small, idiomatic equivalent.

use serde_json::Value;
use thiserror::Error;

/// Errors produced while constructing a module.
#[derive(Debug, Error)]
pub enum ModuleError {
    /// The module's configuration was invalid.
    #[error("invalid module config: {0}")]
    InvalidConfig(String),

    /// The requested hook was not found on this module.
    #[error("module does not provide hook '{0}'")]
    UnknownHook(String),

    /// Any other error.
    #[error("{0}")]
    Other(String),
}

/// A Prebid Server module.
pub trait Module: Send + Sync {
    /// The unique name of the module (e.g. `"vendor.module_name"`).
    fn name(&self) -> &str;
}

/// A factory responsible for producing a [`Module`] from its host-level
/// configuration.
pub trait ModuleBuilder: Send + Sync {
    /// The name of the module this builder produces.
    fn name(&self) -> &str;

    /// Construct a new module instance from the given raw JSON config.
    fn build(&self, config: Option<&Value>) -> Result<Box<dyn Module>, ModuleError>;
}
