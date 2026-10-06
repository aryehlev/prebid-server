//! Go `config.Adapter` / `config.Server`: what `adapters.Builder` receives.

/// Go `config.AdapterXAPI` (Rubicon).
#[derive(Debug, Clone, Default)]
pub struct AdapterXapi {
    pub username: String,
    pub password: String,
    pub tracker: String,
}

/// Go `config.Adapter`.
#[derive(Debug, Clone, Default)]
pub struct Adapter {
    pub endpoint: String,
    pub extra_adapter_info: String,
    /// Rubicon.
    pub xapi: AdapterXapi,
    /// AppNexus and Facebook.
    pub platform_id: String,
    /// Facebook.
    pub app_secret: String,
}

impl Adapter {
    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into(), ..Self::default() }
    }
}

/// Go `config.Server`.
#[derive(Debug, Clone, Default)]
pub struct Server {
    pub external_url: String,
    pub gvl_id: i32,
    pub data_center: String,
}
