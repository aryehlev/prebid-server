//! Macros crate — a Go-template–style macro resolver for PBS macro keys
//! like `##PBS-BIDID##`, `##PBS-APPBUNDLE##`, etc.
//!
//! Ported (in spirit) from the Go `macros` package. The Go implementation uses
//! `text/template` plus a string-index-based replacer keyed on `##KEY##`.
//! Here we provide a lightweight replacer that scans for `##...##` tokens and
//! substitutes values from a [`MacroContext`].

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Standard PBS macro keys (without the surrounding `##`).
pub mod keys {
    pub const BID_ID: &str = "PBS-BIDID";
    pub const APP_BUNDLE: &str = "PBS-APPBUNDLE";
    pub const DOMAIN: &str = "PBS-DOMAIN";
    pub const PUB_DOMAIN: &str = "PBS-PUBDOMAIN";
    pub const PAGE_URL: &str = "PBS-PAGEURL";
    pub const ACCOUNT_ID: &str = "PBS-ACCOUNTID";
    pub const LMT_TRACKING: &str = "PBS-LIMITADTRACKING";
    pub const CONSENT: &str = "PBS-GDPRCONSENT";
    pub const BIDDER: &str = "PBS-BIDDER";
    pub const INTEGRATION: &str = "PBS-INTEGRATION";
    pub const VAST_CRT_ID: &str = "PBS-VASTCRTID";
    pub const TIMESTAMP: &str = "PBS-TIMESTAMP";
    pub const AUCTION_ID: &str = "PBS-AUCTIONID";
    pub const CHANNEL: &str = "PBS-CHANNEL";
    pub const EVENT_TYPE: &str = "PBS-EVENTTYPE";
    pub const VAST_EVENT: &str = "PBS-VASTEVENT";

    /// Prefix for custom macros defined by publishers/integrations.
    pub const CUSTOM_MACRO_PREFIX: &str = "PBS-MACRO-";
}

/// Maximum length for custom macro values (matches Go `customMacroLength`).
pub const CUSTOM_MACRO_LENGTH: usize = 100;

/// Context holding all macro values available for substitution.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MacroContext {
    pub bid_id: Option<String>,
    pub app_bundle: Option<String>,
    pub domain: Option<String>,
    pub pub_domain: Option<String>,
    pub page_url: Option<String>,
    pub account_id: Option<String>,
    pub limit_ad_tracking: Option<String>,
    pub gdpr_consent: Option<String>,
    pub bidder: Option<String>,
    pub integration: Option<String>,
    pub vast_creative_id: Option<String>,
    pub timestamp: Option<String>,
    pub auction_id: Option<String>,
    pub channel: Option<String>,
    pub event_type: Option<String>,
    pub vast_event: Option<String>,
    /// Custom macros. Keys are stored WITHOUT the `PBS-MACRO-` prefix.
    pub custom: HashMap<String, String>,
}

impl MacroContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a custom macro. The value is truncated to [`CUSTOM_MACRO_LENGTH`]
    /// chars, matching the Go implementation.
    pub fn set_custom(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let v = value.into();
        let truncated = truncate(&v, CUSTOM_MACRO_LENGTH);
        self.custom.insert(key.into(), truncated);
    }

    /// Lookup a macro by its bare key (e.g. `PBS-BIDID`).
    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            keys::BID_ID => self.bid_id.clone(),
            keys::APP_BUNDLE => self.app_bundle.clone(),
            keys::DOMAIN => self.domain.clone(),
            keys::PUB_DOMAIN => self.pub_domain.clone(),
            keys::PAGE_URL => self.page_url.clone(),
            keys::ACCOUNT_ID => self.account_id.clone(),
            keys::LMT_TRACKING => self.limit_ad_tracking.clone(),
            keys::CONSENT => self.gdpr_consent.clone(),
            keys::BIDDER => self.bidder.clone(),
            keys::INTEGRATION => self.integration.clone(),
            keys::VAST_CRT_ID => self.vast_creative_id.clone(),
            keys::TIMESTAMP => self.timestamp.clone(),
            keys::AUCTION_ID => self.auction_id.clone(),
            keys::CHANNEL => self.channel.clone(),
            keys::EVENT_TYPE => self.event_type.clone(),
            keys::VAST_EVENT => self.vast_event.clone(),
            other => {
                if let Some(custom_key) = other.strip_prefix(keys::CUSTOM_MACRO_PREFIX) {
                    self.custom.get(custom_key).cloned()
                } else {
                    None
                }
            }
        }
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < max_chars {
        s.to_string()
    } else {
        chars.into_iter().take(max_chars).collect()
    }
}

/// Trait for resolving macros in a string template.
pub trait StringReplacer {
    /// Replace all macros in `template` with values from `ctx`.
    fn resolve(&self, template: &str, ctx: &MacroContext) -> String;
}

/// Default `##KEY##`-style replacer.
///
/// Scans the input for the delimiter `##`, extracts the key between the two
/// delimiters, and replaces the full token with its value from the context.
/// If the key is unknown, the token is left unchanged (matching the lenient
/// behaviour of the Go string-index-based replacer).
#[derive(Debug, Default, Clone, Copy)]
pub struct HashHashReplacer;

impl HashHashReplacer {
    pub fn new() -> Self {
        Self
    }
}

const DELIM: &str = "##";

impl StringReplacer for HashHashReplacer {
    fn resolve(&self, template: &str, ctx: &MacroContext) -> String {
        let mut out = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(start) = rest.find(DELIM) {
            out.push_str(&rest[..start]);
            let after_start = &rest[start + DELIM.len()..];
            match after_start.find(DELIM) {
                Some(end) => {
                    let key = &after_start[..end];
                    match ctx.get(key) {
                        Some(val) => out.push_str(&val),
                        None => {
                            // Unknown key — keep token verbatim.
                            out.push_str(DELIM);
                            out.push_str(key);
                            out.push_str(DELIM);
                        }
                    }
                    rest = &after_start[end + DELIM.len()..];
                }
                None => {
                    // Dangling delimiter, emit as-is and stop.
                    out.push_str(DELIM);
                    out.push_str(after_start);
                    return out;
                }
            }
        }
        out.push_str(rest);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ctx() -> MacroContext {
        let mut c = MacroContext::new();
        c.bid_id = Some("bid-123".into());
        c.app_bundle = Some("com.example.app".into());
        c.domain = Some("example.com".into());
        c.pub_domain = Some("pub.example.com".into());
        c.page_url = Some("https://example.com/page".into());
        c.account_id = Some("acct-42".into());
        c.bidder = Some("some-bidder".into());
        c.vast_creative_id = Some("creative-9".into());
        c.set_custom("FOO", "bar");
        c
    }

    #[test]
    fn replaces_known_macros() {
        let r = HashHashReplacer::new();
        let ctx = sample_ctx();
        let tpl = "bid=##PBS-BIDID## acct=##PBS-ACCOUNTID## domain=##PBS-DOMAIN##";
        let out = r.resolve(tpl, &ctx);
        assert_eq!(out, "bid=bid-123 acct=acct-42 domain=example.com");
    }

    #[test]
    fn leaves_unknown_macros_verbatim() {
        let r = HashHashReplacer::new();
        let ctx = MacroContext::new();
        // Both keys are unset in the empty context, so both are treated
        // as unknown and preserved verbatim.
        let tpl = "x=##PBS-BIDID## y=##UNKNOWN##";
        let out = r.resolve(tpl, &ctx);
        assert_eq!(out, "x=##PBS-BIDID## y=##UNKNOWN##");
    }

    #[test]
    fn custom_macro_prefix_and_truncation() {
        let mut ctx = MacroContext::new();
        let long = "x".repeat(150);
        ctx.set_custom("BIG", &long);
        let stored = ctx.get("PBS-MACRO-BIG").unwrap();
        assert_eq!(stored.len(), CUSTOM_MACRO_LENGTH);
    }

    #[test]
    fn handles_no_macros() {
        let r = HashHashReplacer::new();
        let ctx = MacroContext::new();
        assert_eq!(r.resolve("hello world", &ctx), "hello world");
    }

    #[test]
    fn handles_dangling_delim() {
        let r = HashHashReplacer::new();
        let ctx = MacroContext::new();
        assert_eq!(r.resolve("foo ##bar", &ctx), "foo ##bar");
    }

    #[test]
    fn multiple_replacements_and_adjacent() {
        let r = HashHashReplacer::new();
        let mut ctx = MacroContext::new();
        ctx.bidder = Some("abc".into());
        ctx.account_id = Some("99".into());
        let out = r.resolve("##PBS-BIDDER####PBS-ACCOUNTID##", &ctx);
        assert_eq!(out, "abc99");
    }
}
