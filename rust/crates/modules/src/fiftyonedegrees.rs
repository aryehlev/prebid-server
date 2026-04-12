//! Stub for the `fiftyonedegrees` device-detection hook module.
//!
//! Provides a minimal [`detect_device`] entry point so downstream code can be
//! wired and exercised before the real 51Degrees integration is available.

use serde::{Deserialize, Serialize};

/// A very thin wrapper around a User-Agent string. We use a newtype so the
/// public API is stable even once we switch to a richer representation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserAgent(pub String);

impl UserAgent {
    pub fn new(ua: impl Into<String>) -> Self {
        Self(ua.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Device class identified from a user agent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceClass {
    #[default]
    Unknown,
    Phone,
    Tablet,
    Desktop,
    Tv,
    Bot,
}

/// Result of a user-agent lookup.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_class: DeviceClass,
    pub make: String,
    pub model: String,
    pub os: String,
    pub os_version: String,
    pub browser: String,
    pub is_mobile: bool,
}

/// Module configuration — typically loaded from host.yaml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FiftyOneDegreesConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub data_file: String,
    #[serde(default)]
    pub account_id: String,
}

/// The module handle. Owning the config lets the stub call into
/// [`detect_device`] as a method as well.
pub struct FiftyOneDegreesModule {
    pub config: FiftyOneDegreesConfig,
}

impl FiftyOneDegreesModule {
    pub fn new(config: FiftyOneDegreesConfig) -> Self {
        Self { config }
    }
}

/// Stub device-detection entry point. This uses extremely naive substring
/// matching — the real module will shell out to the 51Degrees SDK.
pub fn detect_device(ua: &UserAgent) -> DeviceInfo {
    let s = ua.as_str();
    let lower = s.to_ascii_lowercase();

    let mut info = DeviceInfo::default();

    if lower.contains("bot") || lower.contains("crawler") || lower.contains("spider") {
        info.device_class = DeviceClass::Bot;
        return info;
    }

    if lower.contains("ipad") || lower.contains("tablet") {
        info.device_class = DeviceClass::Tablet;
        info.is_mobile = true;
    } else if lower.contains("iphone") || lower.contains("android") || lower.contains("mobile") {
        info.device_class = DeviceClass::Phone;
        info.is_mobile = true;
    } else if lower.contains("smart-tv") || lower.contains("smarttv") {
        info.device_class = DeviceClass::Tv;
    } else if !lower.is_empty() {
        info.device_class = DeviceClass::Desktop;
    }

    if lower.contains("iphone") {
        info.make = "Apple".to_string();
        info.model = "iPhone".to_string();
        info.os = "iOS".to_string();
    } else if lower.contains("ipad") {
        info.make = "Apple".to_string();
        info.model = "iPad".to_string();
        info.os = "iPadOS".to_string();
    } else if lower.contains("android") {
        info.os = "Android".to_string();
    } else if lower.contains("windows") {
        info.os = "Windows".to_string();
    } else if lower.contains("mac os") || lower.contains("macintosh") {
        info.os = "macOS".to_string();
    }

    if lower.contains("chrome") {
        info.browser = "Chrome".to_string();
    } else if lower.contains("firefox") {
        info.browser = "Firefox".to_string();
    } else if lower.contains("safari") {
        info.browser = "Safari".to_string();
    } else if lower.contains("edge") {
        info.browser = "Edge".to_string();
    }

    info
}

impl FiftyOneDegreesModule {
    pub fn detect(&self, ua: &UserAgent) -> DeviceInfo {
        if !self.config.enabled {
            return DeviceInfo::default();
        }
        detect_device(ua)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_phone() {
        let ua = UserAgent::new("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) Safari");
        let info = detect_device(&ua);
        assert_eq!(info.device_class, DeviceClass::Phone);
        assert!(info.is_mobile);
        assert_eq!(info.make, "Apple");
        assert_eq!(info.os, "iOS");
        assert_eq!(info.browser, "Safari");
    }

    #[test]
    fn test_detect_tablet() {
        let ua = UserAgent::new("Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X)");
        let info = detect_device(&ua);
        assert_eq!(info.device_class, DeviceClass::Tablet);
        assert!(info.is_mobile);
    }

    #[test]
    fn test_detect_desktop_chrome() {
        let ua = UserAgent::new("Mozilla/5.0 (Windows NT 10.0; Win64) Chrome/120.0");
        let info = detect_device(&ua);
        assert_eq!(info.device_class, DeviceClass::Desktop);
        assert!(!info.is_mobile);
        assert_eq!(info.os, "Windows");
        assert_eq!(info.browser, "Chrome");
    }

    #[test]
    fn test_detect_bot() {
        let ua = UserAgent::new("Googlebot/2.1 (+http://www.google.com/bot.html)");
        let info = detect_device(&ua);
        assert_eq!(info.device_class, DeviceClass::Bot);
    }

    #[test]
    fn test_module_disabled_returns_empty() {
        let m = FiftyOneDegreesModule::new(FiftyOneDegreesConfig::default());
        let info = m.detect(&UserAgent::new("iPhone"));
        assert_eq!(info.device_class, DeviceClass::Unknown);
    }
}
