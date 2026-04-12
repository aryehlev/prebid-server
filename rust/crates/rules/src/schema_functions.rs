//! Built-in schema functions and the canonical [`RequestCtx`] used by tests
//! and the default rules engine wiring.
//!
//! Mirrors a small subset of Go `rules/schema_functions.go`.

use crate::{RulesError, SchemaFunction};

/// A simple request context used by the built-in schema functions. In the
/// real system this would be an `openrtb_ext::RequestWrapper`, but for the
/// purposes of the port we use a plain struct so the `rules` crate has no
/// internal dependencies.
#[derive(Debug, Default, Clone)]
pub struct RequestCtx {
    pub device_country: String,
    pub channel: String,
    pub device_type: String,
}

// ---------------------------------------------------------------------------
// deviceCountry
// ---------------------------------------------------------------------------

/// Returns the device country code (e.g. `"US"`).
pub struct DeviceCountry;

impl SchemaFunction<RequestCtx> for DeviceCountry {
    fn name(&self) -> &str {
        "deviceCountry"
    }
    fn call(&self, ctx: &RequestCtx) -> Result<String, RulesError> {
        if ctx.device_country.is_empty() {
            Ok("unknown".to_string())
        } else {
            Ok(ctx.device_country.clone())
        }
    }
}

// ---------------------------------------------------------------------------
// channel
// ---------------------------------------------------------------------------

/// Returns the request channel (`"web"`, `"app"`, etc.).
pub struct Channel;

impl SchemaFunction<RequestCtx> for Channel {
    fn name(&self) -> &str {
        "channel"
    }
    fn call(&self, ctx: &RequestCtx) -> Result<String, RulesError> {
        if ctx.channel.is_empty() {
            Ok("unknown".to_string())
        } else {
            Ok(ctx.channel.clone())
        }
    }
}

// ---------------------------------------------------------------------------
// deviceType
// ---------------------------------------------------------------------------

/// Returns the device type (`"phone"`, `"tablet"`, `"desktop"`, ...).
pub struct DeviceType;

impl SchemaFunction<RequestCtx> for DeviceType {
    fn name(&self) -> &str {
        "deviceType"
    }
    fn call(&self, ctx: &RequestCtx) -> Result<String, RulesError> {
        if ctx.device_type.is_empty() {
            Ok("unknown".to_string())
        } else {
            Ok(ctx.device_type.clone())
        }
    }
}
