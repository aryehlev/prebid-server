//! Account configuration lookup for the Prebid Server Rust port.
//!
//! Mirrors the Go `account` package: resolves an account id through a pluggable
//! fetcher, merges it with global defaults, and rejects disabled accounts.

pub mod defaults;
pub mod error;
pub mod fetcher;
pub mod get_account;
pub mod types;

pub use defaults::{merge_defaults, AccountDefaults};
pub use error::AccountError;
pub use fetcher::AccountFetcher;
pub use get_account::get_account;
pub use types::{Account, AccountPrivacy};
