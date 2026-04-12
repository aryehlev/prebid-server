//! Syncer abstractions and template-based URL substitution.
//!
//! Ported from `usersync/syncer.go` and `usersync/synctype.go`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Supported sync response formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SyncType {
    /// Iframe response format.
    Iframe,
    /// HTTP 302 redirect response format.
    Redirect,
}

impl SyncType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncType::Iframe => "iframe",
            SyncType::Redirect => "redirect",
        }
    }
}

/// A user sync to be performed by the user's device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sync {
    /// Fully-rendered sync URL with macros substituted.
    pub url: String,
    /// The sync type used to produce this URL.
    pub sync_type: SyncType,
    /// Whether CORS must be supported for this URL.
    pub support_cors: bool,
}

/// Filter representing which sync types are acceptable for a given sync request.
#[derive(Debug, Clone, Default)]
pub struct SyncTypeFilter {
    /// Acceptable sync types.
    pub allowed: Vec<SyncType>,
}

impl SyncTypeFilter {
    pub fn all() -> Self {
        Self {
            allowed: vec![SyncType::Iframe, SyncType::Redirect],
        }
    }

    pub fn iframe_only() -> Self {
        Self {
            allowed: vec![SyncType::Iframe],
        }
    }

    pub fn redirect_only() -> Self {
        Self {
            allowed: vec![SyncType::Redirect],
        }
    }

    pub fn allows(&self, sync_type: SyncType) -> bool {
        self.allowed.contains(&sync_type)
    }
}

/// Privacy policy macro values substituted into sync URLs.
#[derive(Debug, Clone, Default)]
pub struct PrivacyPolicies {
    pub gdpr: String,
    pub gdpr_consent: String,
    pub us_privacy: String,
    pub gpp: String,
    pub gpp_sid: String,
    pub redirect_url: String,
}

/// Errors returned when producing a [`Sync`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SyncerError {
    #[error("no sync types provided")]
    NoSyncTypesProvided,
    #[error("no sync types supported")]
    NoSyncTypesSupported,
}

/// Represents the user sync configuration for a bidder (or shared set of bidders).
pub trait Syncer: Send + std::marker::Sync {
    /// The syncer key as stored in the user's cookie.
    fn key(&self) -> &str;

    /// The default sync type for this syncer.
    fn default_sync_type(&self) -> SyncType;

    /// Returns true if this syncer supports at least one of the provided sync types.
    fn supports_type(&self, sync_types: &[SyncType]) -> bool;

    /// Build a [`Sync`] for this syncer, subject to the provided filter and
    /// privacy policy macro values.
    fn get_sync(
        &self,
        sync_type_filter: &SyncTypeFilter,
        privacy: &PrivacyPolicies,
    ) -> Result<Sync, SyncerError>;
}

/// A basic template-backed [`Syncer`] implementation. The iframe/redirect URL
/// templates are simple strings containing Go-style `{{.Name}}` macros.
pub struct StandardSyncer {
    key: String,
    default_sync_type: SyncType,
    iframe_template: Option<String>,
    redirect_template: Option<String>,
    support_cors: bool,
}

impl StandardSyncer {
    pub fn new(
        key: impl Into<String>,
        default_sync_type: SyncType,
        iframe_template: Option<String>,
        redirect_template: Option<String>,
    ) -> Self {
        Self {
            key: key.into(),
            default_sync_type,
            iframe_template,
            redirect_template,
            support_cors: false,
        }
    }

    pub fn with_support_cors(mut self, support_cors: bool) -> Self {
        self.support_cors = support_cors;
        self
    }

    fn filter_supported(&self, sync_types: &[SyncType]) -> Vec<SyncType> {
        sync_types
            .iter()
            .copied()
            .filter(|t| match t {
                SyncType::Iframe => self.iframe_template.is_some(),
                SyncType::Redirect => self.redirect_template.is_some(),
            })
            .collect()
    }

    fn template_for(&self, sync_type: SyncType) -> Option<&str> {
        match sync_type {
            SyncType::Iframe => self.iframe_template.as_deref(),
            SyncType::Redirect => self.redirect_template.as_deref(),
        }
    }
}

impl Syncer for StandardSyncer {
    fn key(&self) -> &str {
        &self.key
    }

    fn default_sync_type(&self) -> SyncType {
        self.default_sync_type
    }

    fn supports_type(&self, sync_types: &[SyncType]) -> bool {
        !self.filter_supported(sync_types).is_empty()
    }

    fn get_sync(
        &self,
        sync_type_filter: &SyncTypeFilter,
        privacy: &PrivacyPolicies,
    ) -> Result<Sync, SyncerError> {
        if sync_type_filter.allowed.is_empty() {
            return Err(SyncerError::NoSyncTypesProvided);
        }
        let supported = self.filter_supported(&sync_type_filter.allowed);
        if supported.is_empty() {
            return Err(SyncerError::NoSyncTypesSupported);
        }

        // Prefer the syncer's default type if it is among the supported set.
        let chosen = if supported.contains(&self.default_sync_type) {
            self.default_sync_type
        } else {
            supported[0]
        };

        let template = self.template_for(chosen).expect("filtered to supported");
        let url = resolve_macros(template, privacy);

        Ok(Sync {
            url,
            sync_type: chosen,
            support_cors: self.support_cors,
        })
    }
}

/// Substitute the Prebid Server privacy policy macros in a URL template.
///
/// Handled macros: `{{.GDPR}}`, `{{.GDPRConsent}}`, `{{.USPrivacy}}`, `{{.GPP}}`,
/// `{{.GPPSID}}`, `{{.RedirectURL}}`. Whitespace between the braces and the
/// identifier is tolerated (e.g. `{{ .GDPR }}`).
pub fn resolve_macros(template: &str, privacy: &PrivacyPolicies) -> String {
    let replacements: [(&str, &str); 6] = [
        ("GDPR", privacy.gdpr.as_str()),
        ("GDPRConsent", privacy.gdpr_consent.as_str()),
        ("USPrivacy", privacy.us_privacy.as_str()),
        ("GPP", privacy.gpp.as_str()),
        ("GPPSID", privacy.gpp_sid.as_str()),
        ("RedirectURL", privacy.redirect_url.as_str()),
    ];

    // Walk the template once and replace macros in order. We match a very small
    // subset of Go's text/template syntax: `{{` optional_ws `.` NAME optional_ws `}}`.
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            // Find the closing `}}`.
            if let Some(end) = find_close(&bytes[i + 2..]) {
                let inner = &template[i + 2..i + 2 + end];
                let trimmed = inner.trim();
                if let Some(name) = trimmed.strip_prefix('.') {
                    let name = name.trim();
                    if let Some((_, value)) = replacements.iter().find(|(k, _)| *k == name) {
                        out.push_str(value);
                        i += 2 + end + 2;
                        continue;
                    }
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn find_close(haystack: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i + 1 < haystack.len() {
        if haystack[i] == b'}' && haystack[i + 1] == b'}' {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policies() -> PrivacyPolicies {
        PrivacyPolicies {
            gdpr: "1".into(),
            gdpr_consent: "CONSENT-STR".into(),
            us_privacy: "1YNN".into(),
            gpp: "gpp-str".into(),
            gpp_sid: "2_6".into(),
            redirect_url: "https://example.com/setuid?bidder=x".into(),
        }
    }

    #[test]
    fn substitutes_all_macros() {
        let tmpl = "https://host/sync?gdpr={{.GDPR}}&consent={{.GDPRConsent}}&usp={{.USPrivacy}}&gpp={{.GPP}}&sid={{.GPPSID}}&r={{.RedirectURL}}";
        let out = resolve_macros(tmpl, &policies());
        assert_eq!(
            out,
            "https://host/sync?gdpr=1&consent=CONSENT-STR&usp=1YNN&gpp=gpp-str&sid=2_6&r=https://example.com/setuid?bidder=x"
        );
    }

    #[test]
    fn tolerates_whitespace_in_macro() {
        let tmpl = "{{ .GDPR }}-{{.GDPRConsent}}";
        assert_eq!(resolve_macros(tmpl, &policies()), "1-CONSENT-STR");
    }

    #[test]
    fn leaves_unknown_macros_untouched() {
        let tmpl = "abc {{.Unknown}} def";
        assert_eq!(resolve_macros(tmpl, &policies()), "abc {{.Unknown}} def");
    }

    #[test]
    fn standard_syncer_get_sync_prefers_default() {
        let syncer = StandardSyncer::new(
            "acme",
            SyncType::Redirect,
            Some("https://i/{{.GDPR}}".into()),
            Some("https://r/{{.GDPR}}".into()),
        );
        let filter = SyncTypeFilter::all();
        let sync = syncer.get_sync(&filter, &policies()).unwrap();
        assert_eq!(sync.sync_type, SyncType::Redirect);
        assert_eq!(sync.url, "https://r/1");
    }

    #[test]
    fn standard_syncer_falls_back_when_default_unsupported() {
        let syncer = StandardSyncer::new(
            "acme",
            SyncType::Iframe,
            None,
            Some("https://r/{{.GDPR}}".into()),
        );
        let sync = syncer
            .get_sync(&SyncTypeFilter::all(), &policies())
            .unwrap();
        assert_eq!(sync.sync_type, SyncType::Redirect);
    }

    #[test]
    fn standard_syncer_rejects_empty_filter() {
        let syncer = StandardSyncer::new(
            "acme",
            SyncType::Iframe,
            Some("https://i".into()),
            None,
        );
        let filter = SyncTypeFilter { allowed: vec![] };
        assert_eq!(
            syncer.get_sync(&filter, &policies()),
            Err(SyncerError::NoSyncTypesProvided)
        );
    }

    #[test]
    fn standard_syncer_rejects_unsupported_filter() {
        let syncer = StandardSyncer::new(
            "acme",
            SyncType::Iframe,
            Some("https://i".into()),
            None,
        );
        assert_eq!(
            syncer.get_sync(&SyncTypeFilter::redirect_only(), &policies()),
            Err(SyncerError::NoSyncTypesSupported)
        );
    }
}
