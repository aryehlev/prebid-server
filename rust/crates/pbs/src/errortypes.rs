//! Go `errortypes`: the error kinds adapters return from `make_requests` / `make_bids`.
//!
//! `Display` is the Go `Error()` text, which the fixtures compare verbatim.

use std::fmt;

// Go `errortypes/code.go`: `UnknownErrorCode = 999`, then `iota` counted from the start of the
// const group (`TimeoutErrorCode` is its second line, so 1).
pub const UNKNOWN_ERROR_CODE: i32 = 999;
pub const TIMEOUT_ERROR_CODE: i32 = 1;
pub const BAD_INPUT_ERROR_CODE: i32 = 2;
pub const BAD_SERVER_RESPONSE_ERROR_CODE: i32 = 4;
pub const FAILED_TO_REQUEST_BIDS_ERROR_CODE: i32 = 5;
pub const FAILED_TO_MARSHAL_ERROR_CODE: i32 = 13;
pub const FAILED_TO_UNMARSHAL_ERROR_CODE: i32 = 14;
pub const UNKNOWN_WARNING_CODE: i32 = 10999;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BidderError {
    /// Bad user input (Go `errortypes.BadInput`).
    BadInput(String),
    /// Bad response from the bidder (Go `errortypes.BadServerResponse`).
    BadServerResponse(String),
    /// Go `errortypes.FailedToRequestBids`.
    FailedToRequestBids(String),
    /// Go `errortypes.Timeout`.
    Timeout(String),
    /// Go `errortypes.Warning`.
    Warning(String),
    /// Go `errortypes.FailedToMarshal`.
    FailedToMarshal(String),
    /// Go `errortypes.FailedToUnmarshal`.
    FailedToUnmarshal(String),
    /// A plain Go `error` (`errors.New` / `fmt.Errorf`).
    Other(String),
}

impl BidderError {
    pub fn bad_input(msg: impl Into<String>) -> Self {
        Self::BadInput(msg.into())
    }

    pub fn bad_server_response(msg: impl Into<String>) -> Self {
        Self::BadServerResponse(msg.into())
    }

    pub fn failed_to_request_bids(msg: impl Into<String>) -> Self {
        Self::FailedToRequestBids(msg.into())
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }

    /// Go `Code()`.
    pub fn code(&self) -> i32 {
        match self {
            Self::BadInput(_) => BAD_INPUT_ERROR_CODE,
            Self::BadServerResponse(_) => BAD_SERVER_RESPONSE_ERROR_CODE,
            Self::FailedToRequestBids(_) => FAILED_TO_REQUEST_BIDS_ERROR_CODE,
            Self::Timeout(_) => TIMEOUT_ERROR_CODE,
            Self::Warning(_) => UNKNOWN_WARNING_CODE,
            Self::FailedToMarshal(_) => FAILED_TO_MARSHAL_ERROR_CODE,
            Self::FailedToUnmarshal(_) => FAILED_TO_UNMARSHAL_ERROR_CODE,
            Self::Other(_) => UNKNOWN_ERROR_CODE,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::BadInput(m)
            | Self::BadServerResponse(m)
            | Self::FailedToRequestBids(m)
            | Self::Timeout(m)
            | Self::Warning(m)
            | Self::FailedToMarshal(m)
            | Self::FailedToUnmarshal(m)
            | Self::Other(m) => m,
        }
    }
}

impl fmt::Display for BidderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for BidderError {}
