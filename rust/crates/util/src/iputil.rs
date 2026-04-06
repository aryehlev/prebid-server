//! IP address parsing, validation, and masking utilities.
//! Mirrors Go `util/iputil` package.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// IPVersion is the numerical version of the IP address spec (4 or 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IpVersion {
    Unknown,
    V4,
    V6,
}

pub const IPV4_BIT_SIZE: u8 = 32;
pub const IPV6_BIT_SIZE: u8 = 128;
pub const IPV4_DEFAULT_MASKING_BIT_SIZE: u8 = 24;
pub const IPV6_DEFAULT_MASKING_BIT_SIZE: u8 = 56;

/// Parse an IP address string, returning the parsed address and its version.
pub fn parse_ip(v: &str) -> (Option<IpAddr>, IpVersion) {
    match v.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => (Some(IpAddr::V4(ip)), IpVersion::V4),
        Ok(IpAddr::V6(ip)) => {
            if v.contains(':') {
                (Some(IpAddr::V6(ip)), IpVersion::V6)
            } else {
                (Some(IpAddr::V6(ip)), IpVersion::V4)
            }
        }
        Err(_) => (None, IpVersion::Unknown),
    }
}

/// IPValidator trait — equivalent to Go's `iputil.IPValidator` interface.
pub trait IpValidator {
    fn is_valid(&self, ip: &IpAddr, ver: IpVersion) -> bool;
}

/// Validates that an IP is not in known private networks.
pub struct PublicNetworkIpValidator {
    pub ipv4_private_networks: Vec<ipnet::Ipv4Net>,
    pub ipv6_private_networks: Vec<ipnet::Ipv6Net>,
}

/// Validates an IP based on a desired version.
pub struct VersionIpValidator {
    pub version: IpVersion,
}

impl IpValidator for VersionIpValidator {
    fn is_valid(&self, _ip: &IpAddr, ver: IpVersion) -> bool {
        ver == self.version
    }
}

/// Sanitize an IP address: strip IPv6 prefix and port information.
pub fn sanitize_ip(ip: &str) -> String {
    let ip = ip.trim();

    // Strip IPv6-mapped IPv4 prefix
    let ip = if let Some(stripped) = ip.strip_prefix("::ffff:") {
        stripped
    } else {
        ip
    };

    // Handle IPv6 addresses with port: [::1]:port
    if ip.starts_with('[') {
        if let Some(end) = ip.find(']') {
            return ip[1..end].to_string();
        }
    }

    // Handle IPv4 with port: 1.2.3.4:port
    let colon_count = ip.chars().filter(|&c| c == ':').count();
    if colon_count == 1 {
        if let Some(pos) = ip.rfind(':') {
            return ip[..pos].to_string();
        }
    }

    ip.to_string()
}

/// Mask an IPv4 address, keeping only the first `bits_to_keep` bits.
pub fn mask_ipv4(ip: &str, bits_to_keep: u8) -> Option<String> {
    let addr: Ipv4Addr = ip.parse().ok()?;
    if bits_to_keep >= 32 {
        return Some(addr.to_string());
    }
    if bits_to_keep == 0 {
        return Some("0.0.0.0".to_string());
    }
    let raw: u32 = u32::from(addr);
    let mask: u32 = !0u32 << (32 - bits_to_keep);
    let masked = Ipv4Addr::from(raw & mask);
    Some(masked.to_string())
}

/// Mask an IPv6 address, keeping only the first `bits_to_keep` bits.
pub fn mask_ipv6(ip: &str, bits_to_keep: u8) -> Option<String> {
    let addr: Ipv6Addr = ip.parse().ok()?;
    if bits_to_keep >= 128 {
        return Some(addr.to_string());
    }
    if bits_to_keep == 0 {
        return Some("::".to_string());
    }
    let raw: u128 = u128::from(addr);
    let mask: u128 = !0u128 << (128 - bits_to_keep);
    let masked = Ipv6Addr::from(raw & mask);
    Some(masked.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ip_v4() {
        let (ip, ver) = parse_ip("1.2.3.4");
        assert!(ip.is_some());
        assert_eq!(ver, IpVersion::V4);
    }

    #[test]
    fn test_parse_ip_v6() {
        let (ip, ver) = parse_ip("2001:db8::1");
        assert!(ip.is_some());
        assert_eq!(ver, IpVersion::V6);
    }

    #[test]
    fn test_parse_ip_invalid() {
        let (ip, ver) = parse_ip("not_an_ip");
        assert!(ip.is_none());
        assert_eq!(ver, IpVersion::Unknown);
    }

    #[test]
    fn test_sanitize_ip() {
        assert_eq!(sanitize_ip("1.2.3.4"), "1.2.3.4");
        assert_eq!(sanitize_ip("1.2.3.4:80"), "1.2.3.4");
        assert_eq!(sanitize_ip("::ffff:1.2.3.4"), "1.2.3.4");
        assert_eq!(sanitize_ip("[::1]:8080"), "::1");
        assert_eq!(sanitize_ip("2001:db8::1"), "2001:db8::1");
    }

    #[test]
    fn test_mask_ipv4() {
        assert_eq!(mask_ipv4("192.168.1.100", 24), Some("192.168.1.0".to_string()));
        assert_eq!(mask_ipv4("192.168.1.100", 16), Some("192.168.0.0".to_string()));
        assert_eq!(mask_ipv4("192.168.1.100", 32), Some("192.168.1.100".to_string()));
        assert_eq!(mask_ipv4("192.168.1.100", 0), Some("0.0.0.0".to_string()));
        assert_eq!(mask_ipv4("not_an_ip", 24), None);
    }

    #[test]
    fn test_mask_ipv6() {
        assert_eq!(mask_ipv6("2001:db8::1", 32), Some("2001:db8::".to_string()));
        assert_eq!(mask_ipv6("2001:db8::1", 128), Some("2001:db8::1".to_string()));
        assert_eq!(mask_ipv6("2001:db8::1", 0), Some("::".to_string()));
        assert_eq!(mask_ipv6("not_an_ip", 64), None);
    }
}
