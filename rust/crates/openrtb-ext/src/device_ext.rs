//! Prebid-specific extensions for `Device.ext`.
//!
//! Mirrors `openrtb_ext/device.go` (`ExtDevice`, `ExtDevicePrebid`,
//! `ExtDeviceInt`).

use serde::{Deserialize, Serialize};

/// `ExtDevice` — `device.ext`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtDevice {
    /// iOS app tracking status (`atts`). 0..=3 per SKAdNetwork spec.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub atts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prebid: Option<ExtDevicePrebid>,
}

/// `ExtDevicePrebid` — `device.ext.prebid`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtDevicePrebid {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interstitial: Option<ExtDeviceInt>,
}

/// `ExtDeviceInt` — `device.ext.prebid.interstitial`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtDeviceInt {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minwidthperc: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minheightperc: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ext_device() {
        let d = ExtDevice {
            atts: Some(3),
            prebid: Some(ExtDevicePrebid {
                interstitial: Some(ExtDeviceInt {
                    minwidthperc: Some(50),
                    minheightperc: Some(60),
                }),
            }),
        };
        let s = serde_json::to_string(&d).unwrap();
        let parsed: ExtDevice = serde_json::from_str(&s).unwrap();
        assert_eq!(d, parsed);
    }
}
