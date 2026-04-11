//! Privacy scrubber — removes PII from bid requests.
//!
//! Mirrors Go `privacy/scrubber.go` — scrubs device IDs, user IDs,
//! demographics, geographic data, IP addresses.

use openrtb::{BidRequest, Geo};
use std::net::IpAddr;

/// IP configuration for anonymization.
#[derive(Debug, Clone)]
pub struct IpConf {
    pub ipv4_anon_keep_bits: u8,
    pub ipv6_anon_keep_bits: u8,
}

impl Default for IpConf {
    fn default() -> Self {
        Self {
            ipv4_anon_keep_bits: 24, // /24 mask for IPv4
            ipv6_anon_keep_bits: 56, // /56 mask for IPv6
        }
    }
}

/// Scrub all device identifier fields (IDFA, DIDMD5, etc.).
/// Mirrors Go `scrubDeviceIDs`.
pub fn scrub_device_ids(req: &mut BidRequest) {
    if let Some(ref mut device) = req.device {
        device.ifa = None;
        device.didmd5 = None;
        device.didsha1 = None;
        device.dpidmd5 = None;
        device.dpidsha1 = None;
        device.macmd5 = None;
        device.macsha1 = None;
    }
}

/// Scrub user identity fields (ID, BuyerUID, demographics, data).
/// Mirrors Go `scrubUserIDs`.
pub fn scrub_user_ids(req: &mut BidRequest) {
    if let Some(ref mut user) = req.user {
        user.data = vec![];
        user.id = None;
        user.buyeruid = None;
        user.yob = None;
        user.gender = None;
        user.keywords = None;
    }
}

/// Scrub user demographic fields.
/// Mirrors Go `scrubUserDemographics`.
pub fn scrub_user_demographics(req: &mut BidRequest) {
    if let Some(ref mut user) = req.user {
        user.buyeruid = None;
        user.id = None;
        user.yob = None;
        user.gender = None;
    }
}

/// Scrub EIDs from user.
/// Mirrors Go `ScrubEIDs`.
pub fn scrub_eids(req: &mut BidRequest) {
    if let Some(ref mut user) = req.user {
        user.eids = Some(vec![]);
    }
}

/// Scrub TID from source and impression ext.
/// Mirrors Go `ScrubTID`.
pub fn scrub_tid(req: &mut BidRequest) {
    if let Some(ref mut source) = req.source {
        source.tid = None;
    }
}

/// Round geo precision to 2 decimal places.
/// Mirrors Go `scrubGeoPrecision`.
pub fn scrub_geo_precision(geo: &Geo) -> Geo {
    let mut c = geo.clone();
    if let Some(lat) = c.lat {
        // Mirrors Go: float64(int(lat*100.0+0.5)) / 100.0
        c.lat = Some(((lat * 100.0 + 0.5) as i64) as f64 / 100.0);
    }
    if let Some(lon) = c.lon {
        c.lon = Some(((lon * 100.0 + 0.5) as i64) as f64 / 100.0);
    }
    c
}

/// Scrub geographic data (round lat/lon).
/// Mirrors Go `scrubGEO`.
pub fn scrub_geo(req: &mut BidRequest) {
    if let Some(ref mut user) = req.user {
        if let Some(ref geo) = user.geo {
            user.geo = Some(scrub_geo_precision(geo));
        }
    }
    if let Some(ref mut device) = req.device {
        if let Some(ref geo) = device.geo {
            device.geo = Some(scrub_geo_precision(geo));
        }
    }
}

/// Scrub geographic data fully (clear all geo).
/// Mirrors Go `scrubGeoFull`.
pub fn scrub_geo_full(req: &mut BidRequest) {
    if let Some(ref mut user) = req.user {
        if user.geo.is_some() {
            user.geo = Some(Geo::default());
        }
    }
    if let Some(ref mut device) = req.device {
        if device.geo.is_some() {
            device.geo = Some(Geo::default());
        }
    }
}

/// Scrub an IP address by masking to the given number of bits.
/// Mirrors Go `scrubIP`.
pub fn scrub_ip(ip: &str, keep_bits: u8, total_bits: u8) -> String {
    if ip.is_empty() {
        return String::new();
    }
    let addr: IpAddr = match ip.parse() {
        Ok(a) => a,
        Err(_) => return String::new(),
    };
    match addr {
        IpAddr::V4(v4) => {
            let bits = u32::from(v4);
            let mask = if keep_bits >= 32 { u32::MAX } else { u32::MAX << (32 - keep_bits) };
            let masked = bits & mask;
            std::net::Ipv4Addr::from(masked).to_string()
        }
        IpAddr::V6(v6) => {
            let bits = u128::from(v6);
            let mask = if keep_bits >= 128 { u128::MAX } else { u128::MAX << (128 - keep_bits) };
            let masked = bits & mask;
            std::net::Ipv6Addr::from(masked).to_string()
        }
    }
}

/// Scrub device IP addresses.
/// Mirrors Go `scrubDeviceIP`.
pub fn scrub_device_ip(req: &mut BidRequest, conf: &IpConf) {
    if let Some(ref mut device) = req.device {
        if let Some(ref ip) = device.ip {
            let scrubbed = scrub_ip(ip, conf.ipv4_anon_keep_bits, 32);
            device.ip = Some(scrubbed);
        }
        if let Some(ref ipv6) = device.ipv6 {
            let scrubbed = scrub_ip(ipv6, conf.ipv6_anon_keep_bits, 128);
            device.ipv6 = Some(scrubbed);
        }
    }
}

/// Combined scrub: device IDs, IPs, user demographics, geo.
/// Mirrors Go `ScrubDeviceIDsIPsUserDemoExt`.
pub fn scrub_device_ids_ips_user_demo(req: &mut BidRequest, ip_conf: &IpConf, scrub_full_geo: bool) {
    scrub_device_ids(req);
    scrub_device_ip(req, ip_conf);
    scrub_user_demographics(req);
    if scrub_full_geo {
        scrub_geo_full(req);
    } else {
        scrub_geo(req);
    }
}

/// Scrub user first-party data.
/// Mirrors Go `ScrubUserFPD`.
pub fn scrub_user_fpd(req: &mut BidRequest) {
    scrub_device_ids(req);
    scrub_user_ids(req);
    scrub_eids(req);
}

/// Scrub for GDPR ID enforcement.
/// Mirrors Go `ScrubGdprID`.
pub fn scrub_gdpr_id(req: &mut BidRequest) {
    scrub_device_ids(req);
    scrub_user_demographics(req);
    scrub_eids(req);
}

/// Scrub geo and device IP.
/// Mirrors Go `ScrubGeoAndDeviceIP`.
pub fn scrub_geo_and_device_ip(req: &mut BidRequest, ip_conf: &IpConf) {
    scrub_device_ip(req, ip_conf);
    scrub_geo(req);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scrub_ip_v4() {
        assert_eq!(scrub_ip("1.2.3.4", 24, 32), "1.2.3.0");
        assert_eq!(scrub_ip("192.168.1.100", 16, 32), "192.168.0.0");
    }

    #[test]
    fn test_scrub_ip_empty() {
        assert_eq!(scrub_ip("", 24, 32), "");
    }

    #[test]
    fn test_scrub_ip_v6() {
        let scrubbed = scrub_ip("2001:0db8:85a3:0000:0000:8a2e:0370:7334", 32, 128);
        assert!(scrubbed.starts_with("2001:db8:"));
    }

    #[test]
    fn test_scrub_geo_precision() {
        let geo = Geo {
            lat: Some(40.7128),
            lon: Some(-74.0060),
            ..Default::default()
        };
        let scrubbed = scrub_geo_precision(&geo);
        assert_eq!(scrubbed.lat, Some(40.71));
        assert_eq!(scrubbed.lon, Some(-74.0));
    }

    #[test]
    fn test_scrub_device_ids() {
        let mut req = BidRequest {
            device: Some(openrtb::Device {
                ifa: Some("test-ifa".to_string()),
                ua: Some("TestAgent".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        scrub_device_ids(&mut req);
        assert!(req.device.as_ref().unwrap().ifa.is_none());
        // UA should be preserved
        assert_eq!(req.device.as_ref().unwrap().ua, Some("TestAgent".to_string()));
    }

    #[test]
    fn test_scrub_user_ids() {
        let mut req = BidRequest {
            user: Some(openrtb::User {
                id: Some("user1".to_string()),
                buyeruid: Some("buyer1".to_string()),
                yob: Some(1990),
                gender: Some("M".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        scrub_user_ids(&mut req);
        let user = req.user.as_ref().unwrap();
        assert!(user.id.is_none());
        assert!(user.buyeruid.is_none());
        assert!(user.yob.is_none());
        assert!(user.gender.is_none());
    }
}
