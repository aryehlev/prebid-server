//! Injector crate — injects tracking pixels / scripts into creative markup.
//!
//! Ported (in spirit) from the Go `injector` package, which walks VAST XML
//! and emits `<Impression>`, `<Error>`, and `<TrackingEvents>` children
//! into the appropriate parent elements. This Rust port focuses on the
//! common cases: adding tracking events to VAST XML and adding `<img>` /
//! `<script>` tracking to HTML markup.
//!
//! The crate intentionally has no dependency on the other internal PBS
//! crates so it can be compiled standalone. A very small macro-style
//! substitution helper is provided inline so tracker URLs can contain
//! `##KEY##` placeholders.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub mod macros_support;
pub mod vast;
pub mod html;

pub use html::HtmlInjector;
pub use macros_support::{SimpleMacros, MacroMap};
pub use vast::VastInjector;

/// Event categories that are injected into creatives.
///
/// Mirrors Go's `VASTEvents` struct, but kept general so non-VAST
/// injectors can reuse it for e.g. impression pixels in HTML.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TrackingEvents {
    /// Plain impression pixel URLs.
    pub impressions: Vec<String>,
    /// Error tracking URLs (VAST `<Error>`).
    pub errors: Vec<String>,
    /// VAST `<ClickTracking>` URLs inside `<VideoClicks>`.
    pub video_clicks: Vec<String>,
    /// VAST `<NonLinearClickTracking>` URLs.
    pub non_linear_click_tracking: Vec<String>,
    /// VAST `<CompanionClickThrough>` URLs.
    pub companion_click_through: Vec<String>,
    /// Typed tracking events (e.g. `start`, `firstQuartile`, `complete`).
    pub tracking_events: HashMap<String, Vec<String>>,
}

impl TrackingEvents {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if no tracking URLs have been configured.
    pub fn is_empty(&self) -> bool {
        self.impressions.is_empty()
            && self.errors.is_empty()
            && self.video_clicks.is_empty()
            && self.non_linear_click_tracking.is_empty()
            && self.companion_click_through.is_empty()
            && self.tracking_events.values().all(|v| v.is_empty())
    }
}

/// Core injection trait. Implementations take the raw creative markup,
/// the tracking event URLs, and a macro map, and return the markup with
/// tracking injected.
pub trait TrackerInjector {
    fn inject(&self, markup: &str, events: &TrackingEvents, macros: &MacroMap) -> String;
}
