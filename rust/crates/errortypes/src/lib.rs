//! Error types for Prebid Server, mirroring the Go `errortypes` package.
//!
//! Each error carries a numeric code and a severity level so that callers can
//! distinguish fatal errors from warnings and route them to the correct output
//! (HTTP response, debug log, etc.).

use std::fmt;

// ---------------------------------------------------------------------------
// Severity
// ---------------------------------------------------------------------------

/// Severity represents the severity level of a bid processing error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Unknown,
    /// Fatal bid processing error which prevents a bid response.
    Fatal,
    /// Non-fatal bid processing error where invalid or ambiguous data was ignored.
    Warning,
}

impl Default for Severity {
    fn default() -> Self {
        Severity::Unknown
    }
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Any,
    Debug,
}

impl Default for Scope {
    fn default() -> Self {
        Scope::Any
    }
}

// ---------------------------------------------------------------------------
// Error codes — matches Go errortypes/code.go
// ---------------------------------------------------------------------------

pub const UNKNOWN_ERROR_CODE: i32 = 999;
pub const TIMEOUT_ERROR_CODE: i32 = 1;
pub const BAD_INPUT_ERROR_CODE: i32 = 2;
pub const BLOCKED_APP_ERROR_CODE: i32 = 3;
pub const BAD_SERVER_RESPONSE_ERROR_CODE: i32 = 4;
pub const FAILED_TO_REQUEST_BIDS_ERROR_CODE: i32 = 5;
pub const BIDDER_TEMPORARILY_DISABLED_ERROR_CODE: i32 = 6;
pub const ACCOUNT_DISABLED_ERROR_CODE: i32 = 7;
pub const ACCT_REQUIRED_ERROR_CODE: i32 = 8;
pub const NO_CONVERSION_RATE_ERROR_CODE: i32 = 9;
pub const MALFORMED_ACCT_ERROR_CODE: i32 = 10;
pub const MODULE_REJECTION_ERROR_CODE: i32 = 11;
pub const TMAX_TIMEOUT_ERROR_CODE: i32 = 12;
pub const FAILED_TO_MARSHAL_ERROR_CODE: i32 = 13;
pub const FAILED_TO_UNMARSHAL_ERROR_CODE: i32 = 14;
pub const INVALID_IMP_FIRST_PARTY_DATA_ERROR_CODE: i32 = 15;
pub const BIDDER_TEMPORARILY_THROTTLED_ERROR_CODE: i32 = 16;

// Warning codes — matches Go errortypes/code.go
pub const UNKNOWN_WARNING_CODE: i32 = 10999;
pub const INVALID_PRIVACY_CONSENT_WARNING_CODE: i32 = 10001;
pub const ACCOUNT_LEVEL_DEBUG_DISABLED_WARNING_CODE: i32 = 10002;
pub const BIDDER_LEVEL_DEBUG_DISABLED_WARNING_CODE: i32 = 10003;
pub const DISABLED_CURRENCY_CONVERSION_WARNING_CODE: i32 = 10004;
pub const ALTERNATE_BIDDER_CODE_WARNING_CODE: i32 = 10005;
pub const MULTI_BID_WARNING_CODE: i32 = 10006;
pub const AD_SERVER_TARGETING_WARNING_CODE: i32 = 10007;
pub const BID_ADJUSTMENT_WARNING_CODE: i32 = 10008;
pub const FLOOR_BID_REJECTION_WARNING_CODE: i32 = 10009;
pub const INVALID_BID_RESPONSE_DSA_WARNING_CODE: i32 = 10010;
pub const SEC_COOKIE_DEPRECATION_LEN_WARNING_CODE: i32 = 10011;
pub const SEC_BROWSING_TOPICS_WARNING_CODE: i32 = 10012;
pub const INVALID_USER_EIDS_WARNING_CODE: i32 = 10013;
pub const INVALID_USER_UIDS_WARNING_CODE: i32 = 10014;
pub const TOO_LONG_TARGETING_PREFIX_WARNING_CODE: i32 = 10015;
pub const TOO_SHORT_TARGETING_PREFIX_WARNING_CODE: i32 = 10016;
pub const BIDDER_BLOCKED_BY_PRIVACY_SETTINGS: i32 = 10017;

// ---------------------------------------------------------------------------
// PbsError trait — equivalent to Go's Coder interface
// ---------------------------------------------------------------------------

/// Trait for errors that carry a numeric code and severity.
pub trait PbsError: std::error::Error {
    fn code(&self) -> i32;
    fn severity(&self) -> Severity;
    fn scope(&self) -> Scope {
        Scope::Any
    }
}

/// Read the code from an error, returning `UNKNOWN_ERROR_CODE` if it doesn't implement `PbsError`.
pub fn read_code(err: &(dyn std::error::Error + 'static)) -> i32 {
    if let Some(e) = err.downcast_ref::<BidError>() {
        return e.code();
    }
    UNKNOWN_ERROR_CODE
}

/// Read the scope from an error, returning `Scope::Any` if it doesn't implement `PbsError`.
pub fn read_scope(err: &(dyn std::error::Error + 'static)) -> Scope {
    if let Some(e) = err.downcast_ref::<BidError>() {
        return e.scope();
    }
    Scope::Any
}

// ---------------------------------------------------------------------------
// BidError — unified error type replacing all the individual Go error structs
// ---------------------------------------------------------------------------

/// BidError is a unified error type that covers all the error kinds from Go's errortypes package.
#[derive(Debug, Clone)]
pub struct BidError {
    pub message: String,
    pub kind: BidErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BidErrorKind {
    Timeout,
    TmaxTimeout,
    BadInput,
    BlockedApp,
    AccountDisabled,
    AcctRequired,
    BadServerResponse,
    FailedToRequestBids,
    BidderTemporarilyDisabled,
    BidderThrottled,
    MalformedAcct,
    Warning(i32),
    DebugWarning(i32),
    FailedToUnmarshal,
    FailedToMarshal,
    InvalidImpFirstPartyData,
}

impl BidError {
    pub fn new(kind: BidErrorKind, message: impl Into<String>) -> Self {
        BidError {
            message: message.into(),
            kind,
        }
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::Timeout, message)
    }

    pub fn tmax_timeout(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::TmaxTimeout, message)
    }

    pub fn bad_input(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::BadInput, message)
    }

    pub fn blocked_app(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::BlockedApp, message)
    }

    pub fn account_disabled(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::AccountDisabled, message)
    }

    pub fn acct_required(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::AcctRequired, message)
    }

    pub fn bad_server_response(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::BadServerResponse, message)
    }

    pub fn failed_to_request_bids(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::FailedToRequestBids, message)
    }

    pub fn bidder_temporarily_disabled(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::BidderTemporarilyDisabled, message)
    }

    pub fn bidder_throttled(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::BidderThrottled, message)
    }

    pub fn malformed_acct(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::MalformedAcct, message)
    }

    pub fn warning(code: i32, message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::Warning(code), message)
    }

    pub fn debug_warning(code: i32, message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::DebugWarning(code), message)
    }

    pub fn failed_to_unmarshal(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::FailedToUnmarshal, message)
    }

    pub fn failed_to_marshal(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::FailedToMarshal, message)
    }

    pub fn invalid_imp_first_party_data(message: impl Into<String>) -> Self {
        Self::new(BidErrorKind::InvalidImpFirstPartyData, message)
    }
}

impl fmt::Display for BidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for BidError {}

impl PbsError for BidError {
    fn code(&self) -> i32 {
        match self.kind {
            BidErrorKind::Timeout => TIMEOUT_ERROR_CODE,
            BidErrorKind::TmaxTimeout => TMAX_TIMEOUT_ERROR_CODE,
            BidErrorKind::BadInput => BAD_INPUT_ERROR_CODE,
            BidErrorKind::BlockedApp => BLOCKED_APP_ERROR_CODE,
            BidErrorKind::AccountDisabled => ACCOUNT_DISABLED_ERROR_CODE,
            BidErrorKind::AcctRequired => ACCT_REQUIRED_ERROR_CODE,
            BidErrorKind::BadServerResponse => BAD_SERVER_RESPONSE_ERROR_CODE,
            BidErrorKind::FailedToRequestBids => FAILED_TO_REQUEST_BIDS_ERROR_CODE,
            BidErrorKind::BidderTemporarilyDisabled => BIDDER_TEMPORARILY_DISABLED_ERROR_CODE,
            BidErrorKind::BidderThrottled => BIDDER_TEMPORARILY_THROTTLED_ERROR_CODE,
            BidErrorKind::MalformedAcct => MALFORMED_ACCT_ERROR_CODE,
            BidErrorKind::Warning(code) => code,
            BidErrorKind::DebugWarning(code) => code,
            BidErrorKind::FailedToUnmarshal => FAILED_TO_UNMARSHAL_ERROR_CODE,
            BidErrorKind::FailedToMarshal => FAILED_TO_MARSHAL_ERROR_CODE,
            BidErrorKind::InvalidImpFirstPartyData => INVALID_IMP_FIRST_PARTY_DATA_ERROR_CODE,
        }
    }

    fn severity(&self) -> Severity {
        match self.kind {
            BidErrorKind::BidderTemporarilyDisabled
            | BidErrorKind::BidderThrottled
            | BidErrorKind::Warning(_)
            | BidErrorKind::DebugWarning(_) => Severity::Warning,
            _ => Severity::Fatal,
        }
    }

    fn scope(&self) -> Scope {
        match self.kind {
            BidErrorKind::DebugWarning(_) => Scope::Debug,
            _ => Scope::Any,
        }
    }
}

// ---------------------------------------------------------------------------
// AggregateError — matches Go errortypes/aggregate.go
// ---------------------------------------------------------------------------

/// AggregateError represents one or more errors.
#[derive(Debug)]
pub struct AggregateError {
    pub message: String,
    pub errors: Vec<Box<dyn std::error::Error + Send + Sync>>,
}

impl AggregateError {
    pub fn new(message: impl Into<String>, errors: Vec<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        AggregateError {
            message: message.into(),
            errors,
        }
    }
}

impl fmt::Display for AggregateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.errors.is_empty() {
            return Ok(());
        }
        write!(f, "{} ({} error", self.message, self.errors.len())?;
        if self.errors.len() != 1 {
            write!(f, "s")?;
        }
        writeln!(f, "):")?;
        for (i, err) in self.errors.iter().enumerate() {
            writeln!(f, "  {}: {}", i + 1, err)?;
        }
        Ok(())
    }
}

impl std::error::Error for AggregateError {}

// ---------------------------------------------------------------------------
// Helper functions — matches Go errortypes/severity.go
// ---------------------------------------------------------------------------

fn is_fatal(err: &BidError) -> bool {
    err.severity() == Severity::Fatal
}

/// Returns true if the error has Warning severity.
pub fn is_warning(err: &BidError) -> bool {
    err.severity() == Severity::Warning
}

/// Checks if the error list contains a fatal error.
pub fn contains_fatal_error(errors: &[BidError]) -> bool {
    errors.iter().any(|e| is_fatal(e))
}

/// Returns a new error list with only the fatal severity errors.
pub fn fatal_only(errors: &[BidError]) -> Vec<&BidError> {
    errors.iter().filter(|e| is_fatal(e)).collect()
}

/// Returns a new error list with only the warning severity errors.
pub fn warning_only(errors: &[BidError]) -> Vec<&BidError> {
    errors.iter().filter(|e| is_warning(e)).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bid_error_timeout() {
        let err = BidError::timeout("request timed out");
        assert_eq!(err.code(), TIMEOUT_ERROR_CODE);
        assert_eq!(err.severity(), Severity::Fatal);
        assert_eq!(err.scope(), Scope::Any);
        assert_eq!(err.to_string(), "request timed out");
    }

    #[test]
    fn test_bid_error_warning() {
        let err = BidError::warning(ALTERNATE_BIDDER_CODE_WARNING_CODE, "some warning");
        assert_eq!(err.code(), ALTERNATE_BIDDER_CODE_WARNING_CODE);
        assert_eq!(err.severity(), Severity::Warning);
        assert!(is_warning(&err));
        assert!(!is_fatal(&err));
    }

    #[test]
    fn test_bid_error_debug_warning() {
        let err = BidError::debug_warning(UNKNOWN_WARNING_CODE, "debug only");
        assert_eq!(err.severity(), Severity::Warning);
        assert_eq!(err.scope(), Scope::Debug);
    }

    #[test]
    fn test_bidder_temporarily_disabled() {
        let err = BidError::bidder_temporarily_disabled("bidder x is disabled");
        assert_eq!(err.severity(), Severity::Warning);
        assert_eq!(err.code(), BIDDER_TEMPORARILY_DISABLED_ERROR_CODE);
    }

    #[test]
    fn test_contains_fatal_error() {
        let errors = vec![
            BidError::warning(UNKNOWN_WARNING_CODE, "w1"),
            BidError::bad_input("fatal"),
            BidError::warning(UNKNOWN_WARNING_CODE, "w2"),
        ];
        assert!(contains_fatal_error(&errors));
    }

    #[test]
    fn test_contains_no_fatal_error() {
        let errors = vec![
            BidError::warning(UNKNOWN_WARNING_CODE, "w1"),
            BidError::bidder_temporarily_disabled("disabled"),
        ];
        assert!(!contains_fatal_error(&errors));
    }

    #[test]
    fn test_fatal_only() {
        let errors = vec![
            BidError::warning(UNKNOWN_WARNING_CODE, "w1"),
            BidError::bad_input("fatal"),
            BidError::warning(UNKNOWN_WARNING_CODE, "w2"),
        ];
        let fatals = fatal_only(&errors);
        assert_eq!(fatals.len(), 1);
        assert_eq!(fatals[0].code(), BAD_INPUT_ERROR_CODE);
    }

    #[test]
    fn test_warning_only() {
        let errors = vec![
            BidError::warning(UNKNOWN_WARNING_CODE, "w1"),
            BidError::bad_input("fatal"),
            BidError::warning(UNKNOWN_WARNING_CODE, "w2"),
        ];
        let warnings = warning_only(&errors);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_aggregate_error_display() {
        let agg = AggregateError::new(
            "validation failed",
            vec![
                Box::new(BidError::bad_input("missing field")),
                Box::new(BidError::bad_input("invalid type")),
            ],
        );
        let display = agg.to_string();
        assert!(display.contains("validation failed (2 errors):"));
        assert!(display.contains("1: missing field"));
        assert!(display.contains("2: invalid type"));
    }

    #[test]
    fn test_aggregate_error_empty() {
        let agg = AggregateError::new("empty", vec![]);
        assert_eq!(agg.to_string(), "");
    }

    #[test]
    fn test_all_error_kinds() {
        let cases: Vec<(BidError, i32, Severity)> = vec![
            (BidError::timeout(""), TIMEOUT_ERROR_CODE, Severity::Fatal),
            (BidError::tmax_timeout(""), TMAX_TIMEOUT_ERROR_CODE, Severity::Fatal),
            (BidError::bad_input(""), BAD_INPUT_ERROR_CODE, Severity::Fatal),
            (BidError::blocked_app(""), BLOCKED_APP_ERROR_CODE, Severity::Fatal),
            (BidError::account_disabled(""), ACCOUNT_DISABLED_ERROR_CODE, Severity::Fatal),
            (BidError::acct_required(""), ACCT_REQUIRED_ERROR_CODE, Severity::Fatal),
            (BidError::bad_server_response(""), BAD_SERVER_RESPONSE_ERROR_CODE, Severity::Fatal),
            (BidError::failed_to_request_bids(""), FAILED_TO_REQUEST_BIDS_ERROR_CODE, Severity::Fatal),
            (BidError::bidder_temporarily_disabled(""), BIDDER_TEMPORARILY_DISABLED_ERROR_CODE, Severity::Warning),
            (BidError::bidder_throttled(""), BIDDER_TEMPORARILY_THROTTLED_ERROR_CODE, Severity::Warning),
            (BidError::malformed_acct(""), MALFORMED_ACCT_ERROR_CODE, Severity::Fatal),
            (BidError::failed_to_unmarshal(""), FAILED_TO_UNMARSHAL_ERROR_CODE, Severity::Fatal),
            (BidError::failed_to_marshal(""), FAILED_TO_MARSHAL_ERROR_CODE, Severity::Fatal),
            (BidError::invalid_imp_first_party_data(""), INVALID_IMP_FIRST_PARTY_DATA_ERROR_CODE, Severity::Fatal),
        ];
        for (err, expected_code, expected_severity) in cases {
            assert_eq!(err.code(), expected_code, "code mismatch for {:?}", err.kind);
            assert_eq!(err.severity(), expected_severity, "severity mismatch for {:?}", err.kind);
        }
    }
}
