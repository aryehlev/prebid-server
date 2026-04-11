//! Default value setting for OpenRTB bid requests.
//!
//! Mirrors Go `ortb/default.go` — sets default values for targeting,
//! price granularity, and impression security.

use openrtb::*;

/// Default precision for price granularity.
pub const DEFAULT_PRICE_GRANULARITY_PRECISION: i32 = 2;
/// Default value for include_winners targeting flag.
pub const DEFAULT_TARGETING_INCLUDE_WINNERS: bool = true;
/// Default value for include_bidder_keys targeting flag.
pub const DEFAULT_TARGETING_INCLUDE_BIDDER_KEYS: bool = true;
/// Default secure flag for impressions (1 = HTTPS required).
pub const DEFAULT_SECURE: i32 = 1;

/// Set defaults on impressions: ensure each imp has `secure` set.
/// Mirrors Go `setDefaultsImp`.
pub fn set_defaults_imp(imps: &mut [Imp]) -> bool {
    let mut modified = false;
    for imp in imps.iter_mut() {
        if imp.secure.is_none() {
            imp.secure = Some(DEFAULT_SECURE);
            modified = true;
        }
    }
    modified
}

/// Set default TMax on a bid request if not already set.
pub fn set_default_tmax(req: &mut BidRequest, default_tmax: i64) {
    if req.tmax.is_none() || req.tmax == Some(0) {
        req.tmax = Some(default_tmax);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_defaults_imp_sets_secure() {
        let mut imps = vec![
            Imp { id: "1".to_string(), ..Default::default() },
            Imp { id: "2".to_string(), secure: Some(0), ..Default::default() },
        ];
        let modified = set_defaults_imp(&mut imps);
        assert!(modified);
        assert_eq!(imps[0].secure, Some(1));
        // Already had secure set, should not be changed
        assert_eq!(imps[1].secure, Some(0));
    }

    #[test]
    fn test_set_defaults_imp_no_change() {
        let mut imps = vec![
            Imp { id: "1".to_string(), secure: Some(1), ..Default::default() },
        ];
        let modified = set_defaults_imp(&mut imps);
        assert!(!modified);
    }

    #[test]
    fn test_set_default_tmax() {
        let mut req = BidRequest::default();
        set_default_tmax(&mut req, 3000);
        assert_eq!(req.tmax, Some(3000));
    }

    #[test]
    fn test_set_default_tmax_already_set() {
        let mut req = BidRequest { tmax: Some(5000), ..Default::default() };
        set_default_tmax(&mut req, 3000);
        assert_eq!(req.tmax, Some(5000));
    }
}
