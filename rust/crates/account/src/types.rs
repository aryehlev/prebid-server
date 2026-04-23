use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Privacy configuration block on an account.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AccountPrivacy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gdpr: Option<Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ccpa: Option<Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpp: Option<Value>,

    /// Limit Ad Tracking enforcement config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lmt: Option<Value>,
}

/// Top-level account configuration.
///
/// Complex nested config blocks are kept as `serde_json::Value` so this crate
/// does not need to depend on the full Prebid config crate.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Account {
    #[serde(default)]
    pub id: String,

    #[serde(default)]
    pub disabled: bool,

    #[serde(default, rename = "default_integration", skip_serializing_if = "Option::is_none")]
    pub default_integration: Option<String>,

    /// Cache TTL in seconds (if configured).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_ttl: Option<i64>,

    #[serde(default)]
    pub events_enabled: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie_sync: Option<Value>,

    #[serde(default)]
    pub privacy: AccountPrivacy,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_floors: Option<Value>,

    #[serde(default)]
    pub debug_allow: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analytics: Option<Value>,
}
