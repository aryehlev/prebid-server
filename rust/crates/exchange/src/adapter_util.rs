//! Adapter utility functions.
//! Mirrors Go `exchange/adapter_util.go`.
//!
//! Provides functions for building bidder adapters, retrieving active bidders,
//! and generating disabled bidder warning messages.

use std::collections::HashMap;

/// Get all active (enabled) bidders from the bidder info map.
/// Mirrors Go `GetActiveBidders`.
pub fn get_active_bidders(infos: &HashMap<String, BidderInfo>) -> HashMap<String, String> {
    let mut active = HashMap::new();
    for (name, info) in infos {
        if info.is_enabled() {
            active.insert(name.clone(), name.clone());
        }
    }
    active
}

/// Minimal BidderInfo representation needed for adapter_util functions.
#[derive(Debug, Clone, Default)]
pub struct BidderInfo {
    pub enabled: bool,
    pub disabled: bool,
    pub white_label_only: bool,
    pub alias_of: Option<String>,
    pub endpoint: String,
}

impl BidderInfo {
    pub fn is_enabled(&self) -> bool {
        self.enabled && !self.disabled
    }
}

/// Get warning messages for disabled and removed bidders.
/// Mirrors Go `GetDisabledBidderWarningMessages`.
pub fn get_disabled_bidder_warning_messages(
    infos: &HashMap<String, BidderInfo>,
) -> HashMap<String, String> {
    let mut removed: HashMap<String, String> = HashMap::new();

    // Known removed bidders
    let removed_bidders = [
        ("lifestreet", "Bidder \"lifestreet\" is no longer available in Prebid Server. Please update your configuration."),
        ("somoaudience", "Bidder \"somoaudience\" is no longer available in Prebid Server. Please update your configuration."),
        ("yssp", "Bidder \"yssp\" is no longer available in Prebid Server. If you're looking to use the Yahoo SSP adapter, please rename it to \"yahooAds\" in your configuration."),
        ("andbeyondmedia", "Bidder \"andbeyondmedia\" is no longer available in Prebid Server. If you're looking to use the AndBeyond.Media SSP adapter, please rename it to \"beyondmedia\" in your configuration."),
        ("oftmedia", "Bidder \"oftmedia\" is no longer available in Prebid Server. Please update your configuration."),
        ("groupm", "Bidder \"groupm\" is no longer available in Prebid Server. Please update your configuration."),
        ("verizonmedia", "Bidder \"verizonmedia\" is no longer available in Prebid Server. Please update your configuration."),
        ("brightroll", "Bidder \"brightroll\" is no longer available in Prebid Server. Please update your configuration."),
        ("engagebdr", "Bidder \"engagebdr\" is no longer available in Prebid Server. Please update your configuration."),
        ("ninthdecimal", "Bidder \"ninthdecimal\" is no longer available in Prebid Server. Please update your configuration."),
        ("kubient", "Bidder \"kubient\" is no longer available in Prebid Server. Please update your configuration."),
        ("applogy", "Bidder \"applogy\" is no longer available in Prebid Server. Please update your configuration."),
        ("rhythmone", "Bidder \"rhythmone\" is no longer available in Prebid Server. Please update your configuration."),
        ("nanointeractive", "Bidder \"nanointeractive\" is no longer available in Prebid Server. Please update your configuration."),
        ("bizzclick", "Bidder \"bizzclick\" is no longer available in Prebid Server. Please update your configuration. \"bizzclick\" has been renamed to \"blasto\"."),
        ("liftoff", "Bidder \"liftoff\" is no longer available in Prebid Server. If you're looking to use the Vungle Exchange adapter, please rename it to \"vungle\" in your configuration."),
        ("gothamads", "Bidder \"gothamads\" is no longer available in Prebid Server. Please rename it to \"intenze\" in your configuration."),
        ("intertech", "Bidder \"intertech\" is no longer available in Prebid Server. Please update your configuration."),
        ("adocean", "Bidder \"adocean\" is no longer available in Prebid Server. Please update your configuration."),
        ("dxkulture", "Bidder \"dxkulture\" is no longer available in Prebid Server. Please update your configuration."),
        ("mobupps", "Bidder \"mobupps\" is no longer available in Prebid Server. Please update your configuration."),
        ("vimayx", "Bidder \"vimayx\" is no longer available in Prebid Server. Please update your configuration."),
        ("adoppler", "Bidder \"adoppler\" is no longer available in Prebid Server. If you're looking to use the Adoppler adapter, please rename it to \"elementaltv\" in your configuration."),
    ];

    for (name, msg) in &removed_bidders {
        removed.insert(name.to_string(), msg.to_string());
    }

    merge_removed_and_disabled(removed, infos)
}

fn merge_removed_and_disabled(
    mut disabled_bidders: HashMap<String, String>,
    infos: &HashMap<String, BidderInfo>,
) -> HashMap<String, String> {
    for (name, info) in infos {
        if info.white_label_only {
            disabled_bidders.insert(
                name.clone(),
                format!(
                    "Bidder \"{}\" can only be aliased and cannot be used directly.",
                    name
                ),
            );
        } else if info.disabled {
            disabled_bidders.insert(
                name.clone(),
                format!(
                    "Bidder \"{}\" has been disabled on this instance of Prebid Server. Please work with the PBS host to enable this bidder again.",
                    name
                ),
            );
        }
    }
    disabled_bidders
}

/// Check if a bidder is disabled due to being white-label only.
/// Mirrors Go `IsBidderDisabledDueToWhiteLabelOnly`.
pub fn is_bidder_disabled_due_to_white_label_only(disabled_message: &str) -> bool {
    disabled_message.ends_with("can only be aliased and cannot be used directly.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_active_bidders() {
        let mut infos = HashMap::new();
        infos.insert(
            "appnexus".to_string(),
            BidderInfo {
                enabled: true,
                ..Default::default()
            },
        );
        infos.insert(
            "rubicon".to_string(),
            BidderInfo {
                enabled: true,
                disabled: true,
                ..Default::default()
            },
        );
        infos.insert(
            "pubmatic".to_string(),
            BidderInfo {
                enabled: true,
                ..Default::default()
            },
        );

        let active = get_active_bidders(&infos);
        assert_eq!(active.len(), 2);
        assert!(active.contains_key("appnexus"));
        assert!(active.contains_key("pubmatic"));
        assert!(!active.contains_key("rubicon"));
    }

    #[test]
    fn test_disabled_bidder_warnings_removed() {
        let infos = HashMap::new();
        let warnings = get_disabled_bidder_warning_messages(&infos);
        assert!(warnings.contains_key("lifestreet"));
        assert!(warnings.contains_key("liftoff"));
        assert!(warnings["liftoff"].contains("vungle"));
    }

    #[test]
    fn test_disabled_bidder_warnings_white_label() {
        let mut infos = HashMap::new();
        infos.insert(
            "mywhitelabel".to_string(),
            BidderInfo {
                enabled: true,
                white_label_only: true,
                ..Default::default()
            },
        );

        let warnings = get_disabled_bidder_warning_messages(&infos);
        assert!(warnings.contains_key("mywhitelabel"));
        assert!(warnings["mywhitelabel"].contains("aliased"));
    }

    #[test]
    fn test_is_bidder_disabled_due_to_white_label() {
        assert!(is_bidder_disabled_due_to_white_label_only(
            "Bidder \"x\" can only be aliased and cannot be used directly."
        ));
        assert!(!is_bidder_disabled_due_to_white_label_only(
            "Bidder \"x\" has been disabled."
        ));
    }
}
