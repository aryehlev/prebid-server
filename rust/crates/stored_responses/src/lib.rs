//! Stored responses support for Prebid Server Rust port.
//!
//! Handles extraction of stored auction responses and stored bid responses
//! from incoming bid requests, fetching them via a pluggable fetcher trait,
//! and filtering imps that have stored responses so they do not reach adapters.

pub mod error;
pub mod fetcher;
pub mod filter;
pub mod process;
pub mod types;
pub mod video;

pub use error::StoredRespError;
pub use fetcher::StoredResponsesFetcher;
pub use filter::remove_imps_with_stored_responses;
pub use process::process_stored_responses;
pub use types::{ImpBidderReplaceImpId, ImpBidderStoredResp, ImpsWithBidResponses};
pub use video::flush_stored_video_responses;
