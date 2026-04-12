use async_trait::async_trait;

use crate::error::AccountError;
use crate::types::Account;

/// Pluggable backend for resolving an account id to a stored Account.
#[async_trait]
pub trait AccountFetcher: Send + Sync {
    /// Look up the account with the given id. Implementations should return
    /// [`AccountError::NotFound`] when there is no matching record.
    async fn fetch_account(&self, id: &str) -> Result<Account, AccountError>;
}
