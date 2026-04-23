//! Account fetching and merging logic.
//!
//! This module mirrors the Go `account/account.go` and
//! `stored_requests/backends/file_fetcher` pattern:
//!
//! - **`AccountFetcher`** trait: async interface for retrieving raw account JSON
//!   by publisher ID. Implementations include [`FileAccountFetcher`] (reads from
//!   an `accounts/` directory on disk).
//!
//! - **`get_account`** function: the main entry point that:
//!   1. Fetches the raw JSON for the given account ID
//!   2. Merges the account JSON onto the host-level account defaults using
//!      RFC 7396 JSON Merge Patch
//!   3. Deserializes into an [`AccountConfig`]
//!   4. Validates the account (disabled check)

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use thiserror::Error;

use crate::{AccountConfig, Configuration};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors specific to account fetching and resolution.
#[derive(Debug, Error)]
pub enum AccountError {
    /// The account was not found in the backend.
    #[error("Account not found: {id}")]
    NotFound { id: String },

    /// The server requires a valid account but none was supplied.
    #[error("Prebid-server has been configured to discard requests without a valid Account ID. Please reach out to the prebid server host.")]
    AccountRequired,

    /// The account was found but could not be verified / resolved.
    #[error("Prebid-server could not verify the Account ID. Please reach out to the prebid server host.")]
    AccountVerificationFailed,

    /// The account configuration JSON is malformed.
    #[error("The prebid-server account config for account id \"{id}\" is malformed. Please reach out to the prebid server host.")]
    MalformedAccount { id: String },

    /// The account has been disabled by the host.
    #[error("Prebid-server has disabled Account ID: {id}, please reach out to the prebid server host.")]
    AccountDisabled { id: String },

    /// An internal / backend error occurred while fetching the account.
    #[error("Failed to fetch account \"{id}\": {source}")]
    FetchError {
        id: String,
        source: anyhow::Error,
    },
}

// ---------------------------------------------------------------------------
// AccountFetcher trait
// ---------------------------------------------------------------------------

/// Async trait for fetching raw account JSON by publisher account ID.
///
/// Mirrors Go's `stored_requests.AccountFetcher` interface.
///
/// The `account_defaults_json` parameter contains the serialized host-level
/// account defaults. Implementations that support merge-patch should apply
/// `account_defaults_json` as the base and overlay the account-specific JSON on
/// top before returning the result. Simpler implementations may ignore defaults
/// and return only the account-specific JSON; callers will handle the merge.
#[async_trait]
pub trait AccountFetcher: Send + Sync {
    /// Fetch the merged account JSON for the given `account_id`.
    ///
    /// Returns `Ok(json_bytes)` on success or an appropriate [`AccountError`].
    ///
    /// The `account_defaults_json` is the serialized form of the host
    /// `account_defaults` configuration. Implementations should merge the
    /// account-specific JSON on top of these defaults using RFC 7396 JSON Merge
    /// Patch, matching the Go `jsonpatch.MergePatch(defaults, account)` call.
    async fn fetch_account(
        &self,
        account_defaults_json: Option<&[u8]>,
        account_id: &str,
    ) -> std::result::Result<Vec<u8>, AccountError>;
}

// ---------------------------------------------------------------------------
// FileAccountFetcher
// ---------------------------------------------------------------------------

/// An [`AccountFetcher`] that reads account JSON files from a directory on disk.
///
/// Each account is stored as `{directory}/{account_id}.json`. The file must
/// contain a valid JSON object. On fetch, the account JSON is merged onto the
/// provided defaults using JSON Merge Patch.
///
/// This mirrors Go's `stored_requests/backends/file_fetcher` for accounts.
pub struct FileAccountFetcher {
    /// Pre-loaded account JSON keyed by account ID.
    accounts: HashMap<String, Vec<u8>>,
}

impl FileAccountFetcher {
    /// Create a new [`FileAccountFetcher`] by eagerly reading all `*.json`
    /// files from the given `directory`.
    ///
    /// Each file's stem (filename without `.json`) is used as the account ID.
    pub fn new(directory: &Path) -> Result<Self> {
        if !directory.is_dir() {
            anyhow::bail!(
                "account directory '{}' does not exist or is not a directory",
                directory.display()
            );
        }
        let mut accounts = HashMap::new();
        for entry in std::fs::read_dir(directory)
            .with_context(|| format!("reading account directory '{}'", directory.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let data = std::fs::read(&path).with_context(|| {
                        format!("reading account file '{}'", path.display())
                    })?;
                    // Validate it is parseable JSON.
                    let _: JsonValue = serde_json::from_slice(&data).with_context(|| {
                        format!(
                            "account file '{}' does not contain valid JSON",
                            path.display()
                        )
                    })?;
                    accounts.insert(stem.to_string(), data);
                }
            }
        }
        tracing::info!(
            count = accounts.len(),
            dir = %directory.display(),
            "loaded account files"
        );
        Ok(Self { accounts })
    }

    /// Create an empty [`FileAccountFetcher`] with no pre-loaded accounts.
    ///
    /// Useful for testing or when no accounts directory is configured.
    pub fn empty() -> Self {
        Self {
            accounts: HashMap::new(),
        }
    }
}

#[async_trait]
impl AccountFetcher for FileAccountFetcher {
    async fn fetch_account(
        &self,
        account_defaults_json: Option<&[u8]>,
        account_id: &str,
    ) -> std::result::Result<Vec<u8>, AccountError> {
        if account_id.is_empty() {
            return Err(AccountError::NotFound {
                id: String::new(),
            });
        }

        let account_json = self.accounts.get(account_id).ok_or_else(|| {
            AccountError::NotFound {
                id: account_id.to_string(),
            }
        })?;

        match account_defaults_json {
            Some(defaults) => {
                merge_account_json(defaults, account_json).map_err(|_e| {
                    AccountError::MalformedAccount {
                        id: account_id.to_string(),
                    }
                })
            }
            None => Ok(account_json.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// get_account - the main public API
// ---------------------------------------------------------------------------

/// Fetch, merge, and validate an account by its publisher ID.
///
/// This mirrors the Go `account.GetAccount` function:
///
/// 1. If `account_required` is `true` and `account_id` is empty, return an
///    [`AccountError::AccountRequired`] error.
/// 2. Attempt to fetch the account JSON via the provided [`AccountFetcher`].
/// 3. If not found and `account_required` is true with defaults disabled,
///    return [`AccountError::AccountVerificationFailed`].
/// 4. If not found and not required, return a copy of the host-level
///    `account_defaults` with the given `account_id`.
/// 5. If found, merge with defaults and deserialize.
/// 6. Check if the resolved account is disabled.
///
/// Returns the fully-resolved [`AccountConfig`] on success.
pub async fn get_account(
    cfg: &Configuration,
    fetcher: &dyn AccountFetcher,
    account_id: &str,
) -> std::result::Result<AccountConfig, AccountError> {
    // Step 1: require account ID if configured
    if cfg.account_required && account_id.is_empty() {
        return Err(AccountError::AccountRequired);
    }

    // Serialize account defaults for merge-patch
    let defaults_json = serde_json::to_vec(&cfg.account_defaults).ok();
    let defaults_slice = defaults_json.as_deref();

    // Step 2: fetch
    match fetcher.fetch_account(defaults_slice, account_id).await {
        Ok(merged_json) => {
            // Deserialize the merged JSON into AccountConfig
            let mut account: AccountConfig =
                serde_json::from_slice(&merged_json).map_err(|_e| {
                    AccountError::MalformedAccount {
                        id: account_id.to_string(),
                    }
                })?;

            // Fill in the ID if the account file did not include one
            if account.id.is_empty() {
                account.id = account_id.to_string();
            }

            // Step 6: disabled check
            if account.disabled {
                return Err(AccountError::AccountDisabled {
                    id: account_id.to_string(),
                });
            }

            Ok(account)
        }
        Err(AccountError::NotFound { .. }) => {
            // Account not found in the backend
            if cfg.account_required && cfg.account_defaults.disabled {
                return Err(AccountError::AccountVerificationFailed);
            }

            // Return a copy of the defaults with the requested account ID
            let mut account = cfg.account_defaults.clone();
            account.id = account_id.to_string();

            if account.disabled {
                return Err(AccountError::AccountDisabled {
                    id: account_id.to_string(),
                });
            }

            Ok(account)
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// JSON Merge Patch helper
// ---------------------------------------------------------------------------

/// Merge account-specific JSON onto a defaults JSON document using RFC 7396
/// JSON Merge Patch.
///
/// This mirrors the Go call:
/// ```go
/// completeJSON, err := jsonpatch.MergePatch(accountDefaultsJSON, accountJSON)
/// ```
fn merge_account_json(defaults: &[u8], patch: &[u8]) -> Result<Vec<u8>> {
    let mut base: JsonValue =
        serde_json::from_slice(defaults).context("invalid defaults JSON")?;
    let overlay: JsonValue =
        serde_json::from_slice(patch).context("invalid account JSON")?;
    json_patch::merge(&mut base, &overlay);
    serde_json::to_vec(&base).context("failed to serialize merged account JSON")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // -- merge_account_json --

    #[test]
    fn test_merge_account_json_basic() {
        let defaults = br#"{"disabled": false, "events_enabled": true}"#;
        let account = br#"{"id": "pub123", "disabled": false}"#;
        let merged = merge_account_json(defaults, account).unwrap();
        let val: JsonValue = serde_json::from_slice(&merged).unwrap();
        assert_eq!(val["id"], "pub123");
        assert_eq!(val["disabled"], false);
        assert_eq!(val["events_enabled"], true);
    }

    #[test]
    fn test_merge_account_json_override() {
        let defaults = br#"{"disabled": false, "debug_allow": false}"#;
        let account = br#"{"debug_allow": true}"#;
        let merged = merge_account_json(defaults, account).unwrap();
        let val: JsonValue = serde_json::from_slice(&merged).unwrap();
        assert_eq!(val["debug_allow"], true);
        assert_eq!(val["disabled"], false);
    }

    #[test]
    fn test_merge_account_json_invalid() {
        let defaults = br#"not json"#;
        let account = br#"{"id": "1"}"#;
        assert!(merge_account_json(defaults, account).is_err());
    }

    // -- FileAccountFetcher --

    fn create_temp_accounts_dir(
        accounts: &[(&str, &str)],
    ) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in accounts {
            let path = dir.path().join(format!("{}.json", name));
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(content.as_bytes()).unwrap();
        }
        dir
    }

    #[test]
    fn test_file_account_fetcher_new() {
        let dir = create_temp_accounts_dir(&[
            ("acct1", r#"{"id":"acct1","disabled":false}"#),
            ("acct2", r#"{"id":"acct2","disabled":true}"#),
        ]);
        let fetcher = FileAccountFetcher::new(dir.path()).unwrap();
        assert_eq!(fetcher.accounts.len(), 2);
        assert!(fetcher.accounts.contains_key("acct1"));
        assert!(fetcher.accounts.contains_key("acct2"));
    }

    #[test]
    fn test_file_account_fetcher_missing_dir() {
        let result = FileAccountFetcher::new(Path::new("/tmp/nonexistent_dir_pbs_test_99999"));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_account_fetcher_fetch_found() {
        let dir = create_temp_accounts_dir(&[
            ("pub1", r#"{"id":"pub1","disabled":false}"#),
        ]);
        let fetcher = FileAccountFetcher::new(dir.path()).unwrap();
        let defaults = br#"{"debug_allow":true}"#;
        let result = fetcher.fetch_account(Some(defaults), "pub1").await;
        assert!(result.is_ok());
        let json: JsonValue = serde_json::from_slice(&result.unwrap()).unwrap();
        assert_eq!(json["id"], "pub1");
        assert_eq!(json["debug_allow"], true); // merged from defaults
    }

    #[tokio::test]
    async fn test_file_account_fetcher_fetch_not_found() {
        let fetcher = FileAccountFetcher::empty();
        let result = fetcher.fetch_account(None, "missing").await;
        assert!(matches!(result, Err(AccountError::NotFound { .. })));
    }

    #[tokio::test]
    async fn test_file_account_fetcher_fetch_no_defaults() {
        let dir = create_temp_accounts_dir(&[
            ("pub2", r#"{"id":"pub2","disabled":false}"#),
        ]);
        let fetcher = FileAccountFetcher::new(dir.path()).unwrap();
        let result = fetcher.fetch_account(None, "pub2").await;
        assert!(result.is_ok());
        let json: JsonValue = serde_json::from_slice(&result.unwrap()).unwrap();
        assert_eq!(json["id"], "pub2");
    }

    // -- get_account --

    #[tokio::test]
    async fn test_get_account_found() {
        let dir = create_temp_accounts_dir(&[
            ("pub1", r#"{"id":"pub1","disabled":false}"#),
        ]);
        let fetcher = FileAccountFetcher::new(dir.path()).unwrap();
        let cfg = Configuration::default();

        let account = get_account(&cfg, &fetcher, "pub1").await.unwrap();
        assert_eq!(account.id, "pub1");
        assert!(!account.disabled);
    }

    #[tokio::test]
    async fn test_get_account_not_found_returns_defaults() {
        let fetcher = FileAccountFetcher::empty();
        let cfg = Configuration::default();

        let account = get_account(&cfg, &fetcher, "unknown_pub").await.unwrap();
        assert_eq!(account.id, "unknown_pub");
    }

    #[tokio::test]
    async fn test_get_account_required_empty_id() {
        let fetcher = FileAccountFetcher::empty();
        let mut cfg = Configuration::default();
        cfg.account_required = true;

        let result = get_account(&cfg, &fetcher, "").await;
        assert!(matches!(result, Err(AccountError::AccountRequired)));
    }

    #[tokio::test]
    async fn test_get_account_required_not_found_defaults_disabled() {
        let fetcher = FileAccountFetcher::empty();
        let mut cfg = Configuration::default();
        cfg.account_required = true;
        cfg.account_defaults.disabled = true;

        let result = get_account(&cfg, &fetcher, "some_pub").await;
        assert!(matches!(
            result,
            Err(AccountError::AccountVerificationFailed)
        ));
    }

    #[tokio::test]
    async fn test_get_account_disabled() {
        let dir = create_temp_accounts_dir(&[
            ("disabled_pub", r#"{"id":"disabled_pub","disabled":true}"#),
        ]);
        let fetcher = FileAccountFetcher::new(dir.path()).unwrap();
        let cfg = Configuration::default();

        let result = get_account(&cfg, &fetcher, "disabled_pub").await;
        assert!(matches!(result, Err(AccountError::AccountDisabled { .. })));
    }

    #[tokio::test]
    async fn test_get_account_fills_in_id() {
        // Account file does not include an "id" field.
        let dir = create_temp_accounts_dir(&[
            ("noid", r#"{"disabled":false,"debug_allow":true}"#),
        ]);
        let fetcher = FileAccountFetcher::new(dir.path()).unwrap();
        let cfg = Configuration::default();

        let account = get_account(&cfg, &fetcher, "noid").await.unwrap();
        assert_eq!(account.id, "noid");
        assert!(account.debug_allow);
    }
}
