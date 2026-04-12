//! High-level GPP privacy policy container.
//!
//! Mirrors Go `privacy/gpp` helpers used by the exchange to decide
//! whether a section applies to the current request.

/// Consolidated privacy signals extracted from a bid request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Policy {
    /// Legacy TCF/CCPA consent string.
    pub consent_string: Option<String>,
    /// Full GPP string.
    pub gpp: Option<String>,
    /// Section ids listed as applying (regs.gpp_sid).
    pub gpp_sid: Vec<i32>,
    /// Bidders who should not sell user data (CCPA no-sale list).
    pub no_sale_bidders: Vec<String>,
}

impl Policy {
    /// Returns true if the given GPP section id applies to this request.
    pub fn applies(&self, section_id: i32) -> bool {
        self.gpp_sid.contains(&section_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_checks_sid_list() {
        let p = Policy {
            gpp: Some("DBABMA~1YNN".to_string()),
            gpp_sid: vec![6, 7],
            ..Default::default()
        };
        assert!(p.applies(6));
        assert!(p.applies(7));
        assert!(!p.applies(2));
    }
}
