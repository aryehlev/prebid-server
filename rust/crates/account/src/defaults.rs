use crate::types::Account;

/// Global account defaults applied to every account after fetching.
///
/// Mirrors `config.AccountDefaults` from the Go port: any field set on the
/// defaults object is used as the fallback for an account that does not have
/// the field populated itself.
#[derive(Debug, Clone, Default)]
pub struct AccountDefaults {
    pub account: Account,
}

impl AccountDefaults {
    pub fn new(account: Account) -> Self {
        Self { account }
    }
}

/// Merge `defaults` into `account`, filling in any fields that are unset on
/// `account`. Non-defaulted values already present on `account` are preserved.
pub fn merge_defaults(account: &mut Account, defaults: &AccountDefaults) {
    let d = &defaults.account;

    if !d.disabled {
        // defaults.disabled=false never overrides account; only true defaults propagate.
    } else if !account.disabled {
        account.disabled = true;
    }

    if account.default_integration.is_none() && d.default_integration.is_some() {
        account.default_integration = d.default_integration.clone();
    }

    if account.cache_ttl.is_none() && d.cache_ttl.is_some() {
        account.cache_ttl = d.cache_ttl;
    }

    if !account.events_enabled && d.events_enabled {
        account.events_enabled = true;
    }

    if account.cookie_sync.is_none() && d.cookie_sync.is_some() {
        account.cookie_sync = d.cookie_sync.clone();
    }

    if account.privacy.gdpr.is_none() && d.privacy.gdpr.is_some() {
        account.privacy.gdpr = d.privacy.gdpr.clone();
    }
    if account.privacy.ccpa.is_none() && d.privacy.ccpa.is_some() {
        account.privacy.ccpa = d.privacy.ccpa.clone();
    }
    if account.privacy.gpp.is_none() && d.privacy.gpp.is_some() {
        account.privacy.gpp = d.privacy.gpp.clone();
    }
    if account.privacy.lmt.is_none() && d.privacy.lmt.is_some() {
        account.privacy.lmt = d.privacy.lmt.clone();
    }

    if account.price_floors.is_none() && d.price_floors.is_some() {
        account.price_floors = d.price_floors.clone();
    }

    if !account.debug_allow && d.debug_allow {
        account.debug_allow = true;
    }

    if account.analytics.is_none() && d.analytics.is_some() {
        account.analytics = d.analytics.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AccountPrivacy;
    use serde_json::json;

    #[test]
    fn merge_fills_missing_fields() {
        let mut account = Account {
            id: "acct-1".into(),
            ..Default::default()
        };
        let defaults = AccountDefaults::new(Account {
            default_integration: Some("web".into()),
            cache_ttl: Some(60),
            events_enabled: true,
            privacy: AccountPrivacy {
                gdpr: Some(json!({ "enabled": true })),
                ..Default::default()
            },
            debug_allow: true,
            analytics: Some(json!({ "mod": {} })),
            ..Default::default()
        });

        merge_defaults(&mut account, &defaults);

        assert_eq!(account.default_integration.as_deref(), Some("web"));
        assert_eq!(account.cache_ttl, Some(60));
        assert!(account.events_enabled);
        assert_eq!(account.privacy.gdpr, Some(json!({ "enabled": true })));
        assert!(account.debug_allow);
        assert!(account.analytics.is_some());
    }

    #[test]
    fn merge_does_not_override_present_values() {
        let mut account = Account {
            id: "acct-1".into(),
            default_integration: Some("amp".into()),
            cache_ttl: Some(10),
            ..Default::default()
        };
        let defaults = AccountDefaults::new(Account {
            default_integration: Some("web".into()),
            cache_ttl: Some(60),
            ..Default::default()
        });

        merge_defaults(&mut account, &defaults);

        assert_eq!(account.default_integration.as_deref(), Some("amp"));
        assert_eq!(account.cache_ttl, Some(10));
    }
}
