use crate::defaults::{merge_defaults, AccountDefaults};
use crate::error::AccountError;
use crate::fetcher::AccountFetcher;
use crate::types::Account;

/// Resolve an account id to a fully populated [`Account`].
///
/// Mirrors `account.GetAccount` from the Go port:
///  1. Calls the fetcher to look up the account.
///  2. On `NotFound`, synthesises a copy of the defaults with the supplied id.
///  3. Merges defaults into the fetched account to fill in unset fields.
///  4. Rejects the result if `disabled` is true.
pub async fn get_account(
    fetcher: &dyn AccountFetcher,
    defaults: &AccountDefaults,
    id: &str,
) -> Result<Account, AccountError> {
    let mut account = match fetcher.fetch_account(id).await {
        Ok(mut a) => {
            if a.id.is_empty() {
                a.id = id.to_string();
            }
            a
        }
        Err(AccountError::NotFound(_)) => {
            let mut a = defaults.account.clone();
            a.id = id.to_string();
            a
        }
        Err(other) => return Err(other),
    };

    merge_defaults(&mut account, defaults);

    if account.disabled {
        return Err(AccountError::Disabled(id.to_string()));
    }

    Ok(account)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct StaticFetcher {
        account: Option<Account>,
    }

    #[async_trait]
    impl AccountFetcher for StaticFetcher {
        async fn fetch_account(&self, id: &str) -> Result<Account, AccountError> {
            match &self.account {
                Some(a) => Ok(a.clone()),
                None => Err(AccountError::NotFound(id.to_string())),
            }
        }
    }

    #[tokio::test]
    async fn returns_defaults_when_not_found() {
        let fetcher = StaticFetcher { account: None };
        let defaults = AccountDefaults::new(Account {
            events_enabled: true,
            ..Default::default()
        });
        let acct = get_account(&fetcher, &defaults, "missing").await.unwrap();
        assert_eq!(acct.id, "missing");
        assert!(acct.events_enabled);
    }

    #[tokio::test]
    async fn rejects_disabled_accounts() {
        let fetcher = StaticFetcher {
            account: Some(Account {
                id: "bad".into(),
                disabled: true,
                ..Default::default()
            }),
        };
        let defaults = AccountDefaults::default();
        let err = get_account(&fetcher, &defaults, "bad").await.unwrap_err();
        assert!(matches!(err, AccountError::Disabled(_)));
    }

    #[tokio::test]
    async fn merges_defaults_into_fetched_account() {
        let fetcher = StaticFetcher {
            account: Some(Account {
                id: "acct".into(),
                default_integration: Some("amp".into()),
                ..Default::default()
            }),
        };
        let defaults = AccountDefaults::new(Account {
            cache_ttl: Some(30),
            ..Default::default()
        });
        let acct = get_account(&fetcher, &defaults, "acct").await.unwrap();
        assert_eq!(acct.default_integration.as_deref(), Some("amp"));
        assert_eq!(acct.cache_ttl, Some(30));
    }
}
