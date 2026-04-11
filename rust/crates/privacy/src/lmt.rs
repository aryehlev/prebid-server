//! LMT (Limit Ad Tracking) enforcement.
//!
//! Mirrors Go `privacy/lmt/` package — reads the device.lmt signal
//! and enforces tracking restrictions.

const TRACKING_UNRESTRICTED: i32 = 0;
const TRACKING_RESTRICTED: i32 = 1;

/// Policy represents the LMT (Limit Ad Tracking) policy.
/// Mirrors Go `lmt.Policy`.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    pub signal: i32,
    pub signal_provided: bool,
}

impl Policy {
    /// Read LMT policy from a bid request.
    /// Mirrors Go `lmt.ReadFromRequest`.
    pub fn read_from_request(req: &openrtb::BidRequest) -> Self {
        if let Some(ref device) = req.device {
            if let Some(lmt) = device.lmt {
                return Policy {
                    signal: lmt as i32,
                    signal_provided: true,
                };
            }
        }
        Policy::default()
    }
}

impl super::PolicyEnforcer for Policy {
    fn can_enforce(&self) -> bool {
        self.signal_provided
    }

    fn should_enforce(&self, _bidder: &str) -> bool {
        self.signal_provided && self.signal == TRACKING_RESTRICTED
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PolicyEnforcer;

    #[test]
    fn test_lmt_no_device() {
        let req = openrtb::BidRequest::default();
        let policy = Policy::read_from_request(&req);
        assert!(!policy.can_enforce());
        assert!(!policy.should_enforce("any"));
    }

    #[test]
    fn test_lmt_no_signal() {
        let req = openrtb::BidRequest {
            device: Some(openrtb::Device::default()),
            ..Default::default()
        };
        let policy = Policy::read_from_request(&req);
        assert!(!policy.can_enforce());
    }

    #[test]
    fn test_lmt_restricted() {
        let req = openrtb::BidRequest {
            device: Some(openrtb::Device {
                lmt: Some(1),
                ..Default::default()
            }),
            ..Default::default()
        };
        let policy = Policy::read_from_request(&req);
        assert!(policy.can_enforce());
        assert!(policy.should_enforce("appnexus"));
    }

    #[test]
    fn test_lmt_unrestricted() {
        let req = openrtb::BidRequest {
            device: Some(openrtb::Device {
                lmt: Some(0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let policy = Policy::read_from_request(&req);
        assert!(policy.can_enforce());
        assert!(!policy.should_enforce("appnexus"));
    }
}
