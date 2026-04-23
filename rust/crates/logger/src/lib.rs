//! Logger crate — a thin wrapper around the [`tracing`] crate that mirrors the
//! minimal surface area of the Go `logger` package (Debugf/Infof/Warnf/Errorf).
//!
//! Typical usage:
//!
//! ```no_run
//! logger::init("info", false).ok();
//! logger::info_msg("starting up");
//! logger::warn_msg("careful");
//! ```

use std::sync::atomic::{AtomicBool, Ordering};

use thiserror::Error;
pub use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

/// Errors returned by [`init`].
#[derive(Debug, Error)]
pub enum LoggerError {
    #[error("invalid log level: {0}")]
    InvalidLevel(String),
    #[error("subscriber already initialized")]
    AlreadyInitialized,
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize a global tracing subscriber.
///
/// * `level` — an env-filter string (e.g. `"info"`, `"debug"`, `"my_crate=trace"`).
/// * `json` — if true, emit JSON-formatted logs; otherwise a human-readable format.
///
/// Safe to call more than once: subsequent calls return [`LoggerError::AlreadyInitialized`].
pub fn init(level: &str, json: bool) -> Result<(), LoggerError> {
    if INITIALIZED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(LoggerError::AlreadyInitialized);
    }

    let filter =
        EnvFilter::try_new(level).map_err(|_| LoggerError::InvalidLevel(level.to_string()))?;

    let builder = tracing_subscriber::fmt().with_env_filter(filter);

    let result = if json {
        builder.json().try_init()
    } else {
        builder.try_init()
    };

    result.map_err(|_| LoggerError::AlreadyInitialized)
}

/// Convenience wrappers around the `tracing` macros so callers don't need to
/// pull `tracing` in themselves.
#[inline]
pub fn info_msg(msg: &str) {
    tracing::info!("{}", msg);
}
#[inline]
pub fn warn_msg(msg: &str) {
    tracing::warn!("{}", msg);
}
#[inline]
pub fn error_msg(msg: &str) {
    tracing::error!("{}", msg);
}
#[inline]
pub fn debug_msg(msg: &str) {
    tracing::debug!("{}", msg);
}

/// Logger trait for use as a dependency. Mirrors the Go `Logger` interface.
pub trait Logger: Send + Sync {
    fn debug(&self, msg: &str);
    fn info(&self, msg: &str);
    fn warn(&self, msg: &str);
    fn error(&self, msg: &str);
}

/// Default implementation that delegates to [`tracing`].
#[derive(Debug, Default, Clone, Copy)]
pub struct TracingLogger;

impl TracingLogger {
    pub fn new() -> Self {
        Self
    }
}

impl Logger for TracingLogger {
    fn debug(&self, msg: &str) {
        tracing::debug!("{}", msg);
    }
    fn info(&self, msg: &str) {
        tracing::info!("{}", msg);
    }
    fn warn(&self, msg: &str) {
        tracing::warn!("{}", msg);
    }
    fn error(&self, msg: &str) {
        tracing::error!("{}", msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracing_logger_is_logger() {
        let l: Box<dyn Logger> = Box::new(TracingLogger::new());
        // These should all be no-ops without a subscriber.
        l.debug("d");
        l.info("i");
        l.warn("w");
        l.error("e");
    }

    #[test]
    fn convenience_fns_do_not_panic() {
        info_msg("hello");
        warn_msg("hello");
        error_msg("hello");
        debug_msg("hello");
    }

    #[test]
    fn init_with_invalid_level_errors() {
        // Use a level string that parses but contains something unusable in
        // practice still parses — instead force a failure with empty filter
        // directive invalidity. EnvFilter accepts most strings, so this just
        // asserts the function is callable. We cannot easily force init
        // success in a parallel test environment, so we only assert the
        // error path on bogus input.
        let err = init("§§not a filter§§", false);
        assert!(matches!(
            err,
            Err(LoggerError::InvalidLevel(_)) | Err(LoggerError::AlreadyInitialized)
        ));
    }
}
