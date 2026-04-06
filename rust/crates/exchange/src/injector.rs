//! VAST XML tracker injection.
//!
//! Mirrors Go `injector/injector.go`.
//!
//! Parses VAST XML and injects tracking events (impressions, errors,
//! click tracking, companion click-through, non-linear click tracking)
//! into the appropriate locations in the XML tree.

use std::collections::HashMap;
use std::fmt::Write;

// ---------------------------------------------------------------------------
// Constants — VAST templates and XML tags
// ---------------------------------------------------------------------------

/// Template for a VAST wrapper response when no AdM is present (uses NURL).
const EMPTY_ADM_RESPONSE: &str = r#"<VAST version="3.0"><Ad><Wrapper><AdSystem>prebid.org wrapper</AdSystem><VASTAdTagURI><![CDATA[{nurl}]]></VASTAdTagURI><Creatives></Creatives></Wrapper></Ad></VAST>"#;

// Tag constants
const IMPRESSION_START_TAG: &str = "<Impression><![CDATA[";
const IMPRESSION_END_TAG: &str = "]]></Impression>";
const ERROR_START_TAG: &str = "<Error><![CDATA[";
const ERROR_END_TAG: &str = "]]></Error>";
const CLICK_TRACKING_START_TAG: &str = "<ClickTracking><![CDATA[";
const CLICK_TRACKING_END_TAG: &str = "]]></ClickTracking>";
const NONLINEAR_CLICK_TRACKING_START_TAG: &str = "<NonLinearClickTracking><![CDATA[";
const NONLINEAR_CLICK_TRACKING_END_TAG: &str = "]]></NonLinearClickTracking>";
const COMPANION_CLICK_THROUGH_START_TAG: &str = "<CompanionClickThrough><![CDATA[";
const COMPANION_CLICK_THROUGH_END_TAG: &str = "]]></CompanionClickThrough>";
const TRACKING_EVENTS_START_TAG: &str = "<TrackingEvents>";
const TRACKING_EVENTS_END_TAG: &str = "</TrackingEvents>";
const VIDEO_CLICKS_START_TAG: &str = "<VideoClicks>";
const VIDEO_CLICKS_END_TAG: &str = "</VideoClicks>";
const COMPANION_START_TAG: &str = "<Companion>";
const COMPANION_END_TAG: &str = "</Companion>";
const NONLINEAR_START_TAG: &str = "<NonLinear>";
const NONLINEAR_END_TAG: &str = "</NonLinear>";

// ---------------------------------------------------------------------------
// VASTEvents
// ---------------------------------------------------------------------------

/// Tracking event URLs to inject into VAST XML.
#[derive(Debug, Clone, Default)]
pub struct VASTEvents {
    /// Error tracking URLs.
    pub errors: Vec<String>,
    /// Impression tracking URLs.
    pub impressions: Vec<String>,
    /// Video click tracking URLs.
    pub video_clicks: Vec<String>,
    /// Non-linear click tracking URLs.
    pub non_linear_click_tracking: Vec<String>,
    /// Companion click-through URLs.
    pub companion_click_through: Vec<String>,
    /// Tracking events keyed by event type (e.g., "start", "firstQuartile").
    pub tracking_events: HashMap<String, Vec<String>>,
}

// ---------------------------------------------------------------------------
// InjectionState
// ---------------------------------------------------------------------------

/// Mutable state maintained during VAST XML parsing.
#[derive(Debug, Default)]
struct InjectionState {
    inject_tracker: bool,
    inject_video_clicks: bool,
    inline_wrapper_tag_found: bool,
    wrapper_tag_found: bool,
    impression_tag_found: bool,
    error_tag_found: bool,
    creative_id: String,
    is_creative: bool,
    companion_tag_found: bool,
    non_linear_tag_found: bool,
}

// ---------------------------------------------------------------------------
// TrackerInjector
// ---------------------------------------------------------------------------

/// Injects tracking events into VAST XML.
///
/// Mirrors Go `injector.TrackerInjector`.
pub struct TrackerInjector {
    pub events: VASTEvents,
}

impl TrackerInjector {
    pub fn new(events: VASTEvents) -> Self {
        Self { events }
    }

    /// Inject tracking URLs into VAST XML.
    ///
    /// If `vast_xml` is empty but `nurl` is present, creates a wrapper VAST.
    /// If both are empty, returns an error.
    pub fn inject_tracker(&self, vast_xml: &str, nurl: &str) -> Result<String, String> {
        if vast_xml.is_empty() && nurl.is_empty() {
            return Err("both VAST XML and NURL are empty".to_string());
        }

        if vast_xml.is_empty() {
            // Create wrapper VAST with NURL
            return Ok(EMPTY_ADM_RESPONSE.replace("{nurl}", nurl));
        }

        // Parse and inject into existing VAST XML
        self.process_vast_xml(vast_xml)
    }

    /// Process VAST XML by scanning for element boundaries and injecting
    /// tracking at the appropriate locations.
    ///
    /// This uses simple string-based XML processing rather than a full XML
    /// parser, matching the Go implementation's streaming approach.
    fn process_vast_xml(&self, xml: &str) -> Result<String, String> {
        let mut output = String::with_capacity(xml.len() * 2);
        let mut state = InjectionState::default();
        let mut pos = 0;
        let bytes = xml.as_bytes();

        while pos < bytes.len() {
            if bytes[pos] == b'<' {
                // Find end of tag
                let tag_end = match find_char(bytes, b'>', pos) {
                    Some(e) => e,
                    None => {
                        // Copy remaining
                        output.push_str(&xml[pos..]);
                        break;
                    }
                };

                let tag = &xml[pos..=tag_end];

                if tag.starts_with("</") {
                    // End tag
                    self.handle_end_tag(tag, &mut state, &mut output);
                } else {
                    // Start tag (or self-closing)
                    self.handle_start_tag(tag, &mut state, &mut output);
                }
                pos = tag_end + 1;
            } else {
                // Regular content — copy through
                let next_tag = find_char(bytes, b'<', pos).unwrap_or(bytes.len());
                output.push_str(&xml[pos..next_tag]);
                pos = next_tag;
            }
        }

        if !state.inline_wrapper_tag_found {
            return Err("VAST XML is missing InLine or Wrapper element".to_string());
        }

        Ok(output)
    }

    fn handle_start_tag(&self, tag: &str, state: &mut InjectionState, output: &mut String) {
        let tag_lower = tag.to_lowercase();

        if tag_lower.contains("<wrapper") {
            state.wrapper_tag_found = true;
            output.push_str(tag);
        } else if tag_lower.contains("<creative") {
            state.is_creative = true;
            // Extract adid attribute
            state.creative_id = extract_attribute(tag, "adid").unwrap_or_default();
            output.push_str(tag);
        } else if tag_lower.contains("<linear") && !tag_lower.contains("<nonlinear") {
            state.inject_video_clicks = true;
            state.inject_tracker = true;
            output.push_str(tag);
        } else if tag_lower.contains("<videoclicks") {
            state.inject_video_clicks = false;
            output.push_str(tag);
            // Inject click tracking events inside VideoClicks
            self.add_click_tracking_event(output, &state.creative_id, false);
        } else if tag_lower.contains("<nonlinearads") {
            state.inject_tracker = true;
            output.push_str(tag);
        } else if tag_lower.contains("<trackingevents") && state.is_creative {
            state.inject_tracker = false;
            output.push_str(tag);
            // Inject tracking events inside TrackingEvents
            self.add_tracking_event(output, &state.creative_id, false);
        } else {
            output.push_str(tag);
        }
    }

    fn handle_end_tag(&self, tag: &str, state: &mut InjectionState, output: &mut String) {
        let tag_lower = tag.to_lowercase();

        if tag_lower.contains("</impression") {
            output.push_str(tag);
            if !state.impression_tag_found {
                self.add_impression_tracking_event(output);
            }
            state.impression_tag_found = true;
        } else if tag_lower.contains("</error") {
            output.push_str(tag);
            if !state.error_tag_found {
                self.add_error_tracking_event(output);
            }
            state.error_tag_found = true;
        } else if tag_lower.contains("</nonlinearads") {
            if state.inject_tracker {
                state.inject_tracker = false;
                self.add_tracking_event(output, &state.creative_id, true);
                self.add_non_linear_click_tracking_event(output, &state.creative_id, false);
            }
            output.push_str(tag);
        } else if tag_lower.contains("</linear") && !tag_lower.contains("</nonlinear") {
            if state.inject_video_clicks {
                self.add_click_tracking_event(output, &state.creative_id, true);
                state.inject_video_clicks = false;
            }
            if state.inject_tracker {
                self.add_tracking_event(output, &state.creative_id, true);
                state.inject_tracker = false;
            }
            output.push_str(tag);
        } else if tag_lower.contains("</inline") || tag_lower.contains("</wrapper") {
            state.inline_wrapper_tag_found = true;
            // Inject impression and error tracking if not already found
            if !state.impression_tag_found {
                self.add_impression_tracking_event(output);
            }
            if !state.error_tag_found {
                self.add_error_tracking_event(output);
            }
            output.push_str(tag);
        } else if tag_lower.contains("</nonlinear>") {
            self.add_non_linear_click_tracking_event(output, &state.creative_id, false);
            state.non_linear_tag_found = true;
            output.push_str(tag);
        } else if tag_lower.contains("</companion>") {
            state.companion_tag_found = true;
            self.add_companion_click_through_event(output, &state.creative_id, false);
            output.push_str(tag);
        } else if tag_lower.contains("</companionads") {
            if !state.companion_tag_found && state.wrapper_tag_found {
                self.add_companion_click_through_event(output, &state.creative_id, true);
            }
            output.push_str(tag);
        } else if tag_lower.contains("</creative") {
            state.is_creative = false;
            output.push_str(tag);
        } else {
            output.push_str(tag);
        }
    }

    // -- Injection helpers ---------------------------------------------------

    fn add_tracking_event(&self, output: &mut String, _creative_id: &str, add_parent_tag: bool) {
        if self.events.tracking_events.is_empty() {
            return;
        }
        if add_parent_tag {
            output.push_str(TRACKING_EVENTS_START_TAG);
        }
        for (event_type, urls) in &self.events.tracking_events {
            for url in urls {
                let _ = write!(
                    output,
                    "<Tracking event=\"{}\"><![CDATA[{}]]></Tracking>",
                    event_type, url
                );
            }
        }
        if add_parent_tag {
            output.push_str(TRACKING_EVENTS_END_TAG);
        }
    }

    fn add_click_tracking_event(
        &self,
        output: &mut String,
        _creative_id: &str,
        add_parent_tag: bool,
    ) {
        if self.events.video_clicks.is_empty() {
            return;
        }
        if add_parent_tag {
            output.push_str(VIDEO_CLICKS_START_TAG);
        }
        for url in &self.events.video_clicks {
            output.push_str(CLICK_TRACKING_START_TAG);
            output.push_str(url);
            output.push_str(CLICK_TRACKING_END_TAG);
        }
        if add_parent_tag {
            output.push_str(VIDEO_CLICKS_END_TAG);
        }
    }

    fn add_impression_tracking_event(&self, output: &mut String) {
        for url in &self.events.impressions {
            output.push_str(IMPRESSION_START_TAG);
            output.push_str(url);
            output.push_str(IMPRESSION_END_TAG);
        }
    }

    fn add_error_tracking_event(&self, output: &mut String) {
        for url in &self.events.errors {
            output.push_str(ERROR_START_TAG);
            output.push_str(url);
            output.push_str(ERROR_END_TAG);
        }
    }

    fn add_non_linear_click_tracking_event(
        &self,
        output: &mut String,
        _creative_id: &str,
        add_parent_tag: bool,
    ) {
        if self.events.non_linear_click_tracking.is_empty() {
            return;
        }
        if add_parent_tag {
            output.push_str(NONLINEAR_START_TAG);
        }
        for url in &self.events.non_linear_click_tracking {
            output.push_str(NONLINEAR_CLICK_TRACKING_START_TAG);
            output.push_str(url);
            output.push_str(NONLINEAR_CLICK_TRACKING_END_TAG);
        }
        if add_parent_tag {
            output.push_str(NONLINEAR_END_TAG);
        }
    }

    fn add_companion_click_through_event(
        &self,
        output: &mut String,
        _creative_id: &str,
        add_parent_tag: bool,
    ) {
        if self.events.companion_click_through.is_empty() {
            return;
        }
        if add_parent_tag {
            output.push_str(COMPANION_START_TAG);
        }
        for url in &self.events.companion_click_through {
            output.push_str(COMPANION_CLICK_THROUGH_START_TAG);
            output.push_str(url);
            output.push_str(COMPANION_CLICK_THROUGH_END_TAG);
        }
        if add_parent_tag {
            output.push_str(COMPANION_END_TAG);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn find_char(bytes: &[u8], ch: u8, start: usize) -> Option<usize> {
    for i in start..bytes.len() {
        if bytes[i] == ch {
            return Some(i);
        }
    }
    None
}

/// Extract an attribute value from an XML start tag.
fn extract_attribute(tag: &str, attr_name: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let pattern = format!("{}=", attr_name.to_lowercase());
    let pos = lower.find(&pattern)?;
    let after_eq = &tag[pos + pattern.len()..];
    let (quote, start) = if after_eq.starts_with('"') {
        ('"', 1)
    } else if after_eq.starts_with('\'') {
        ('\'', 1)
    } else {
        return None;
    };
    let end = after_eq[start..].find(quote)?;
    Some(after_eq[start..start + end].to_string())
}

/// Modify VAST XML string by injecting an event impression URL.
///
/// This is used by the events module to inject win/impression tracking.
/// Mirrors Go `events.ModifyVastXmlString`.
pub fn modify_vast_xml_string(vast_xml: &str, impression_url: &str) -> String {
    if vast_xml.is_empty() || impression_url.is_empty() {
        return vast_xml.to_string();
    }

    let events = VASTEvents {
        impressions: vec![impression_url.to_string()],
        ..Default::default()
    };

    let injector = TrackerInjector::new(events);
    match injector.inject_tracker(vast_xml, "") {
        Ok(modified) => modified,
        Err(_) => vast_xml.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_vast() -> String {
        r#"<VAST version="3.0"><Ad><InLine><AdSystem>Test</AdSystem><Impression><![CDATA[https://original.com/imp]]></Impression><Creatives><Creative><Linear><TrackingEvents></TrackingEvents><VideoClicks></VideoClicks></Linear></Creative></Creatives></InLine></Ad></VAST>"#.to_string()
    }

    #[test]
    fn test_inject_empty_both() {
        let injector = TrackerInjector::new(VASTEvents::default());
        assert!(injector.inject_tracker("", "").is_err());
    }

    #[test]
    fn test_inject_empty_vast_with_nurl() {
        let injector = TrackerInjector::new(VASTEvents::default());
        let result = injector.inject_tracker("", "https://nurl.com/vast").unwrap();
        assert!(result.contains("https://nurl.com/vast"));
        assert!(result.contains("VASTAdTagURI"));
    }

    #[test]
    fn test_inject_impression_tracking() {
        let events = VASTEvents {
            impressions: vec!["https://tracker.com/imp".to_string()],
            ..Default::default()
        };
        let injector = TrackerInjector::new(events);
        let result = injector.inject_tracker(&simple_vast(), "").unwrap();
        assert!(result.contains("https://tracker.com/imp"));
        assert!(result.contains("<Impression><![CDATA[https://tracker.com/imp]]></Impression>"));
    }

    #[test]
    fn test_inject_error_tracking() {
        let events = VASTEvents {
            errors: vec!["https://tracker.com/err".to_string()],
            ..Default::default()
        };
        let injector = TrackerInjector::new(events);
        let result = injector.inject_tracker(&simple_vast(), "").unwrap();
        assert!(result.contains("https://tracker.com/err"));
    }

    #[test]
    fn test_inject_click_tracking() {
        let events = VASTEvents {
            video_clicks: vec!["https://tracker.com/click".to_string()],
            ..Default::default()
        };
        let injector = TrackerInjector::new(events);
        let result = injector.inject_tracker(&simple_vast(), "").unwrap();
        assert!(result.contains("https://tracker.com/click"));
    }

    #[test]
    fn test_inject_tracking_events() {
        let mut tracking = HashMap::new();
        tracking.insert(
            "start".to_string(),
            vec!["https://tracker.com/start".to_string()],
        );
        let events = VASTEvents {
            tracking_events: tracking,
            ..Default::default()
        };
        let injector = TrackerInjector::new(events);
        let result = injector.inject_tracker(&simple_vast(), "").unwrap();
        assert!(result.contains("https://tracker.com/start"));
        assert!(result.contains("event=\"start\""));
    }

    #[test]
    fn test_missing_inline_wrapper() {
        let injector = TrackerInjector::new(VASTEvents::default());
        let result = injector.inject_tracker("<VAST><Ad></Ad></VAST>", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_modify_vast_xml_string() {
        let vast = simple_vast();
        let result = modify_vast_xml_string(&vast, "https://event.com/imp");
        assert!(result.contains("https://event.com/imp"));
    }

    #[test]
    fn test_modify_vast_xml_string_empty() {
        assert_eq!(modify_vast_xml_string("", "https://event.com"), "");
        assert_eq!(
            modify_vast_xml_string("<VAST></VAST>", ""),
            "<VAST></VAST>"
        );
    }

    #[test]
    fn test_extract_attribute() {
        assert_eq!(
            extract_attribute(r#"<Creative adid="abc123">"#, "adid"),
            Some("abc123".to_string())
        );
        assert_eq!(
            extract_attribute(r#"<Creative AdId="xyz">"#, "adid"),
            Some("xyz".to_string())
        );
        assert_eq!(extract_attribute("<Creative>", "adid"), None);
    }

    #[test]
    fn test_wrapper_vast() {
        let vast = r#"<VAST version="3.0"><Ad><Wrapper><AdSystem>Test</AdSystem><VASTAdTagURI><![CDATA[https://vast.example.com]]></VASTAdTagURI></Wrapper></Ad></VAST>"#;
        let events = VASTEvents {
            impressions: vec!["https://tracker.com/imp".to_string()],
            ..Default::default()
        };
        let injector = TrackerInjector::new(events);
        let result = injector.inject_tracker(vast, "").unwrap();
        assert!(result.contains("https://tracker.com/imp"));
    }
}
