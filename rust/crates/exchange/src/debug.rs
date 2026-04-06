//! Debug mode support for the exchange layer.
//!
//! Ported from the Go implementation in `exchange/auction.go` and
//! `exchange/utils.go`. Provides structures for collecting debug information
//! during an auction (HTTP calls, request/response bodies) and formatting
//! them for cache storage or inclusion in the bid response.

use regex::Regex;

/// Header name used to pass a debug override token in HTTP requests.
pub const DEBUG_OVERRIDE_HEADER: &str = "x-pbs-debug-override";

// ---------------------------------------------------------------------------
// DebugHttpCall
// ---------------------------------------------------------------------------

/// Captures details of a single HTTP call made to a bidder or external service
/// during the auction. Used to populate `ext.debug.httpcalls` in the response.
#[derive(Debug, Clone, Default)]
pub struct DebugHttpCall {
    /// The URI that was called.
    pub uri: String,
    /// The outgoing request body (may be truncated for very large payloads).
    pub request_body: String,
    /// The response body received.
    pub response_body: String,
    /// HTTP status code of the response, or 0 if the call failed before a
    /// response was received.
    pub status_code: u16,
    /// Wall-clock time in milliseconds the call took.
    pub elapsed_ms: u64,
}

// ---------------------------------------------------------------------------
// DebugData
// ---------------------------------------------------------------------------

/// Raw string data collected during the auction for debug logging. Each field
/// may accumulate content from multiple HTTP calls.
#[derive(Debug, Clone, Default)]
pub struct DebugData {
    /// Serialized bid request(s) sent to bidders.
    pub request: String,
    /// Serialized HTTP headers sent.
    pub headers: String,
    /// Serialized bid response(s) received from bidders.
    pub response: String,
}

// ---------------------------------------------------------------------------
// DebugLog
// ---------------------------------------------------------------------------

/// Collects debug information for an auction request and can build an
/// XML-formatted cache string suitable for storage in Prebid Cache.
///
/// Maps to the Go `DebugLog` struct in `exchange/auction.go`.
#[derive(Debug, Clone)]
pub struct DebugLog {
    /// Whether debug output was requested and is allowed.
    pub enabled: bool,
    /// Cache payload type (e.g. "xml").
    pub cache_type: String,
    /// Collected request/headers/response data.
    pub data: DebugData,
    /// TTL in seconds to use when caching the debug log.
    pub ttl: i64,
    /// Cache key assigned after the debug log is stored.
    pub cache_key: String,
    /// The formatted cache string (built by `build_cache_string`).
    pub cache_string: String,
    /// Optional regex used to scrub sensitive data before caching.
    pub regexp: Option<Regex>,
    /// Whether debug was enabled via the override header rather than the
    /// normal request `test`/`debug` flags.
    pub debug_override: bool,
    /// Cached value of `enabled || debug_override` for fast checks.
    pub debug_enabled_or_overridden: bool,
    /// HTTP calls collected during the auction, keyed by bidder name.
    pub http_calls: Vec<DebugHttpCall>,
}

impl Default for DebugLog {
    fn default() -> Self {
        Self {
            enabled: false,
            cache_type: String::new(),
            data: DebugData::default(),
            ttl: 0,
            cache_key: String::new(),
            cache_string: String::new(),
            regexp: None,
            debug_override: false,
            debug_enabled_or_overridden: false,
            http_calls: Vec::new(),
        }
    }
}

impl DebugLog {
    /// Create a new `DebugLog` with the given enabled state and override flag.
    pub fn new(enabled: bool, debug_override: bool) -> Self {
        Self {
            enabled,
            debug_override,
            debug_enabled_or_overridden: enabled || debug_override,
            ..Default::default()
        }
    }

    /// Build the XML cache string from the collected debug data.
    ///
    /// If a regex is configured, sensitive data matching the pattern is
    /// removed from the request, headers, and response before formatting.
    ///
    /// Mirrors the Go `DebugLog.BuildCacheString()` method.
    pub fn build_cache_string(&mut self) {
        // Apply regex scrubbing if configured.
        if let Some(ref re) = self.regexp {
            self.data.request = re.replace_all(&self.data.request, "").to_string();
            self.data.headers = re.replace_all(&self.data.headers, "").to_string();
            self.data.response = re.replace_all(&self.data.response, "").to_string();
        }

        let request = format!("<Request>{}</Request>", self.data.request);
        let headers = format!("<Headers>{}</Headers>", self.data.headers);
        let response = format!("<Response>{}</Response>", self.data.response);

        self.cache_string = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Log>{}{}{}</Log>",
            request, headers, response
        );
    }

    /// Record a single HTTP call in the debug log.
    pub fn push_http_call(&mut self, call: DebugHttpCall) {
        self.http_calls.push(call);
    }
}

// ---------------------------------------------------------------------------
// get_debug_info
// ---------------------------------------------------------------------------

/// Determine whether debug output should be included in the auction response
/// and whether per-account debug metrics should be recorded.
///
/// Mirrors the Go `getDebugInfo` function in `exchange/utils.go`.
///
/// # Arguments
///
/// * `test` - whether `request.test == 1`
/// * `ext_debug` - whether `request.ext.prebid.debug` is true
/// * `account_debug_flag` - the account's `debug_allow` setting
/// * `debug_log` - mutable reference to the current `DebugLog`
///
/// # Returns
///
/// `(response_debug_allow, account_debug_allow)` tuple:
/// - `response_debug_allow`: if true, include debug output in the response
/// - `account_debug_allow`: if true, record debug metrics for the account
pub fn get_debug_info(
    test: bool,
    ext_debug: bool,
    account_debug_flag: bool,
    debug_log: &mut DebugLog,
) -> (bool, bool) {
    let request_debug_allow = parse_request_debug_values(test, ext_debug);
    set_debug_log_values(account_debug_flag, debug_log);

    let response_debug_allow =
        (request_debug_allow && account_debug_flag) || debug_log.debug_enabled_or_overridden;
    let account_debug_allow =
        (request_debug_allow && account_debug_flag) || (debug_log.debug_enabled_or_overridden && account_debug_flag);

    (response_debug_allow, account_debug_allow)
}

/// Check whether the debug override header value matches the configured token.
///
/// Mirrors the Go `isDebugOverrideEnabled` pattern in the exchange layer.
pub fn is_debug_override_enabled(header_value: &str, configured_token: &str) -> bool {
    !header_value.is_empty()
        && !configured_token.is_empty()
        && header_value == configured_token
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Parse debug flags from the request. Returns true if either `test == 1` or
/// `ext.prebid.debug == true`.
fn parse_request_debug_values(test: bool, ext_debug: bool) -> bool {
    test || ext_debug
}

/// Initialize/update the `DebugLog` enabled state based on the account debug
/// flag.
fn set_debug_log_values(account_debug_flag: bool, debug_log: &mut DebugLog) {
    debug_log.enabled = debug_log.debug_enabled_or_overridden || account_debug_flag;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- DebugLog tests --

    #[test]
    fn test_debug_log_default() {
        let dl = DebugLog::default();
        assert!(!dl.enabled);
        assert!(!dl.debug_override);
        assert!(!dl.debug_enabled_or_overridden);
        assert!(dl.cache_string.is_empty());
        assert!(dl.http_calls.is_empty());
    }

    #[test]
    fn test_debug_log_new_enabled() {
        let dl = DebugLog::new(true, false);
        assert!(dl.enabled);
        assert!(!dl.debug_override);
        assert!(dl.debug_enabled_or_overridden);
    }

    #[test]
    fn test_debug_log_new_override() {
        let dl = DebugLog::new(false, true);
        assert!(!dl.enabled);
        assert!(dl.debug_override);
        assert!(dl.debug_enabled_or_overridden);
    }

    #[test]
    fn test_debug_log_new_both() {
        let dl = DebugLog::new(true, true);
        assert!(dl.enabled);
        assert!(dl.debug_override);
        assert!(dl.debug_enabled_or_overridden);
    }

    #[test]
    fn test_build_cache_string_simple() {
        let mut dl = DebugLog::default();
        dl.data.request = "req-body".to_string();
        dl.data.headers = "Accept: */*".to_string();
        dl.data.response = "resp-body".to_string();

        dl.build_cache_string();

        assert!(dl.cache_string.starts_with("<?xml version=\"1.0\""));
        assert!(dl.cache_string.contains("<Request>req-body</Request>"));
        assert!(dl.cache_string.contains("<Headers>Accept: */*</Headers>"));
        assert!(dl.cache_string.contains("<Response>resp-body</Response>"));
        assert!(dl.cache_string.contains("<Log>"));
        assert!(dl.cache_string.ends_with("</Log>"));
    }

    #[test]
    fn test_build_cache_string_with_regex_scrubbing() {
        let mut dl = DebugLog::default();
        dl.data.request = "secret=abc123&other=ok".to_string();
        dl.data.headers = "Authorization: Bearer secret=xyz".to_string();
        dl.data.response = "no-secret-here".to_string();
        dl.regexp = Some(Regex::new(r"secret=\w+").unwrap());

        dl.build_cache_string();

        assert!(!dl.cache_string.contains("secret=abc123"));
        assert!(!dl.cache_string.contains("secret=xyz"));
        assert!(dl.cache_string.contains("<Request>&amp;other=ok</Request>") ||
                dl.cache_string.contains("<Request>&other=ok</Request>"));
        assert!(dl.cache_string.contains("<Response>no--here</Response>"));
    }

    #[test]
    fn test_build_cache_string_empty_data() {
        let mut dl = DebugLog::default();
        dl.build_cache_string();

        assert_eq!(
            dl.cache_string,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Log><Request></Request><Headers></Headers><Response></Response></Log>"
        );
    }

    #[test]
    fn test_push_http_call() {
        let mut dl = DebugLog::default();
        assert!(dl.http_calls.is_empty());

        dl.push_http_call(DebugHttpCall {
            uri: "https://bidder.example.com/bid".to_string(),
            request_body: r#"{"id":"1"}"#.to_string(),
            response_body: r#"{"seatbid":[]}"#.to_string(),
            status_code: 200,
            elapsed_ms: 42,
        });

        assert_eq!(dl.http_calls.len(), 1);
        assert_eq!(dl.http_calls[0].uri, "https://bidder.example.com/bid");
        assert_eq!(dl.http_calls[0].status_code, 200);
        assert_eq!(dl.http_calls[0].elapsed_ms, 42);
    }

    // -- is_debug_override_enabled tests --

    #[test]
    fn test_is_debug_override_enabled_matching() {
        assert!(is_debug_override_enabled("my-token", "my-token"));
    }

    #[test]
    fn test_is_debug_override_enabled_mismatch() {
        assert!(!is_debug_override_enabled("wrong-token", "my-token"));
    }

    #[test]
    fn test_is_debug_override_enabled_empty_header() {
        assert!(!is_debug_override_enabled("", "my-token"));
    }

    #[test]
    fn test_is_debug_override_enabled_empty_configured() {
        assert!(!is_debug_override_enabled("my-token", ""));
    }

    #[test]
    fn test_is_debug_override_enabled_both_empty() {
        assert!(!is_debug_override_enabled("", ""));
    }

    // -- get_debug_info tests --

    #[test]
    fn test_get_debug_info_test_flag_with_account_allow() {
        let mut dl = DebugLog::default();
        let (resp, acct) = get_debug_info(true, false, true, &mut dl);
        assert!(resp, "response debug should be allowed");
        assert!(acct, "account debug should be allowed");
    }

    #[test]
    fn test_get_debug_info_test_flag_without_account_allow() {
        let mut dl = DebugLog::default();
        let (resp, acct) = get_debug_info(true, false, false, &mut dl);
        assert!(!resp, "response debug should be denied without account flag");
        assert!(!acct, "account debug should be denied");
    }

    #[test]
    fn test_get_debug_info_ext_debug_with_account_allow() {
        let mut dl = DebugLog::default();
        let (resp, acct) = get_debug_info(false, true, true, &mut dl);
        assert!(resp);
        assert!(acct);
    }

    #[test]
    fn test_get_debug_info_override_enables_response_debug() {
        let mut dl = DebugLog::new(false, true); // debug_override = true
        let (resp, acct) = get_debug_info(false, false, false, &mut dl);
        assert!(resp, "override should enable response debug");
        assert!(!acct, "override without account flag should not enable account debug");
    }

    #[test]
    fn test_get_debug_info_override_with_account_flag() {
        let mut dl = DebugLog::new(false, true);
        let (resp, acct) = get_debug_info(false, false, true, &mut dl);
        assert!(resp);
        assert!(acct, "override + account flag should enable account debug");
    }

    #[test]
    fn test_get_debug_info_no_debug_at_all() {
        let mut dl = DebugLog::default();
        let (resp, acct) = get_debug_info(false, false, false, &mut dl);
        assert!(!resp);
        assert!(!acct);
    }

    #[test]
    fn test_get_debug_info_updates_debug_log_enabled() {
        let mut dl = DebugLog::default();
        assert!(!dl.enabled);

        let _ = get_debug_info(false, false, true, &mut dl);
        // set_debug_log_values sets enabled = debug_enabled_or_overridden || account_debug_flag
        assert!(dl.enabled, "debug_log.enabled should be set to true when account debug is true");
    }

    // -- parse_request_debug_values tests --

    #[test]
    fn test_parse_request_debug_values_test_flag() {
        assert!(parse_request_debug_values(true, false));
    }

    #[test]
    fn test_parse_request_debug_values_ext_debug() {
        assert!(parse_request_debug_values(false, true));
    }

    #[test]
    fn test_parse_request_debug_values_both() {
        assert!(parse_request_debug_values(true, true));
    }

    #[test]
    fn test_parse_request_debug_values_neither() {
        assert!(!parse_request_debug_values(false, false));
    }

    // -- DebugHttpCall tests --

    #[test]
    fn test_debug_http_call_default() {
        let call = DebugHttpCall::default();
        assert!(call.uri.is_empty());
        assert!(call.request_body.is_empty());
        assert!(call.response_body.is_empty());
        assert_eq!(call.status_code, 0);
        assert_eq!(call.elapsed_ms, 0);
    }

    // -- DEBUG_OVERRIDE_HEADER constant --

    #[test]
    fn test_debug_override_header_constant() {
        assert_eq!(DEBUG_OVERRIDE_HEADER, "x-pbs-debug-override");
    }
}
