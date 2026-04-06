//! Account fetching and configuration.
//! Mirrors Go `account/account.go`.
//!
//! Provides the `get_account` function that resolves an account ID to a
//! full account configuration, applying defaults and validating the result.

use pbs_config::AccountConfig as Account;
use std::collections::HashMap;

/// Fetch and resolve an account by ID.
/// If the account is not found, falls back to default account configuration.
/// Mirrors Go `GetAccount`.
pub fn get_account(
    cfg_account_required: bool,
    cfg_account_defaults: &Account,
    account_json: Option<&[u8]>,
    account_id: &str,
) -> Result<Account, Vec<String>> {
    // If account is required but ID is missing/unknown
    if cfg_account_required && (account_id.is_empty() || account_id == "unknown") {
        return Err(vec![
            "Prebid-server has been configured to discard requests without a valid Account ID. \
             Please reach out to the prebid server host."
                .to_string(),
        ]);
    }

    let account = match account_json {
        Some(json) => {
            match serde_json::from_slice::<Account>(json) {
                Ok(mut acct) => {
                    if acct.id.is_empty() {
                        acct.id = account_id.to_string();
                    }
                    acct
                }
                Err(_) => {
                    return Err(vec![format!(
                        "The prebid-server account config for account id \"{}\" is malformed. \
                         Please reach out to the prebid server host.",
                        account_id
                    )]);
                }
            }
        }
        None => {
            if cfg_account_required && cfg_account_defaults.disabled {
                return Err(vec![
                    "Prebid-server could not verify the Account ID. \
                     Please reach out to the prebid server host."
                        .to_string(),
                ]);
            }
            let mut acct = cfg_account_defaults.clone();
            acct.id = account_id.to_string();
            acct
        }
    };

    if account.disabled {
        return Err(vec![format!(
            "Prebid-server has disabled Account ID: {}, please reach out to the prebid server host.",
            account_id
        )]);
    }

    Ok(account)
}

/// TCF2 enforcement algorithm identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tcf2EnforcementAlgo {
    Undefined,
    Basic,
    Full,
}

/// Map of enforcement algorithm string names to enum values.
pub fn tcf2_enforcements() -> HashMap<String, Tcf2EnforcementAlgo> {
    let mut m = HashMap::new();
    m.insert("basic".to_string(), Tcf2EnforcementAlgo::Basic);
    m.insert("full".to_string(), Tcf2EnforcementAlgo::Full);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_account_required_missing() {
        let defaults = Account::default();
        let result = get_account(true, &defaults, None, "unknown");
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("valid Account ID"));
    }

    #[test]
    fn test_get_account_defaults_when_not_found() {
        let defaults = Account::default();
        let result = get_account(false, &defaults, None, "acct-123");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, "acct-123");
    }

    #[test]
    fn test_get_account_from_json() {
        let json = br#"{"id": "acct-456"}"#;
        let defaults = Account::default();
        let result = get_account(false, &defaults, Some(json), "acct-456");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, "acct-456");
    }

    #[test]
    fn test_get_account_disabled() {
        let json = br#"{"id": "acct-789", "disabled": true}"#;
        let defaults = Account::default();
        let result = get_account(false, &defaults, Some(json), "acct-789");
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("disabled"));
    }

    #[test]
    fn test_get_account_malformed() {
        let json = b"not valid json";
        let defaults = Account::default();
        let result = get_account(false, &defaults, Some(json), "acct-bad");
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("malformed"));
    }

    #[test]
    fn test_tcf2_enforcements() {
        let m = tcf2_enforcements();
        assert_eq!(m["basic"], Tcf2EnforcementAlgo::Basic);
        assert_eq!(m["full"], Tcf2EnforcementAlgo::Full);
    }
}
