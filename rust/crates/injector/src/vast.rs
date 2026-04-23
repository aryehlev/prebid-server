//! VAST tracker injection.
//!
//! The Go implementation streams the input XML and injects new child
//! elements at specific parent boundaries (e.g. before `</Linear>`).
//! Writing a full streaming XML parser is out of scope for this port,
//! so this Rust version takes a pragmatic string-replacement approach:
//!
//! * `<Impression>…</Impression>` URLs are inserted just before the first
//!   closing `</InLine>` or `</Wrapper>` tag.
//! * `<TrackingEvents>` entries are injected before the closing
//!   `</Linear>` tag. If one already exists, new `<Tracking>` elements
//!   are spliced inside it; otherwise a fresh `<TrackingEvents>` block
//!   is created.
//! * `<Error>` URLs are injected before `</InLine>` / `</Wrapper>`.
//!
//! This preserves enough behaviour to be useful for unit tests and to
//! drive higher-level code, without requiring a full XML streamer.

use super::macros_support::{MacroMap, SimpleMacros};
use super::{TrackerInjector, TrackingEvents};

#[derive(Debug, Default, Clone, Copy)]
pub struct VastInjector {
    macros: SimpleMacros,
}

impl VastInjector {
    pub fn new() -> Self {
        Self {
            macros: SimpleMacros::new(),
        }
    }

    fn build_impressions(&self, events: &TrackingEvents, m: &MacroMap) -> String {
        let mut out = String::new();
        for url in &events.impressions {
            out.push_str("<Impression><![CDATA[");
            out.push_str(&self.macros.resolve(url, m));
            out.push_str("]]></Impression>");
        }
        out
    }

    fn build_errors(&self, events: &TrackingEvents, m: &MacroMap) -> String {
        let mut out = String::new();
        for url in &events.errors {
            out.push_str("<Error><![CDATA[");
            out.push_str(&self.macros.resolve(url, m));
            out.push_str("]]></Error>");
        }
        out
    }

    fn build_tracking_children(&self, events: &TrackingEvents, m: &MacroMap) -> String {
        let mut out = String::new();
        // Stable-ish order: sort keys for deterministic test output.
        let mut keys: Vec<&String> = events.tracking_events.keys().collect();
        keys.sort();
        for k in keys {
            if let Some(urls) = events.tracking_events.get(k) {
                for url in urls {
                    out.push_str(&format!("<Tracking event=\"{}\"><![CDATA[", k));
                    out.push_str(&self.macros.resolve(url, m));
                    out.push_str("]]></Tracking>");
                }
            }
        }
        out
    }

    fn build_tracking_block(&self, events: &TrackingEvents, m: &MacroMap) -> String {
        let inner = self.build_tracking_children(events, m);
        if inner.is_empty() {
            String::new()
        } else {
            format!("<TrackingEvents>{}</TrackingEvents>", inner)
        }
    }
}

fn insert_before(haystack: &str, needle: &str, fragment: &str) -> Option<String> {
    haystack.find(needle).map(|idx| {
        let mut s = String::with_capacity(haystack.len() + fragment.len());
        s.push_str(&haystack[..idx]);
        s.push_str(fragment);
        s.push_str(&haystack[idx..]);
        s
    })
}

impl TrackerInjector for VastInjector {
    fn inject(&self, markup: &str, events: &TrackingEvents, macros: &MacroMap) -> String {
        if markup.is_empty() {
            return markup.to_string();
        }

        let imp_block = self.build_impressions(events, macros);
        let err_block = self.build_errors(events, macros);
        let tracking_block = self.build_tracking_block(events, macros);

        let mut out = markup.to_string();

        // Insert impressions/errors before the first wrapper/inline close.
        let top_fragment = {
            let mut s = String::new();
            s.push_str(&imp_block);
            s.push_str(&err_block);
            s
        };
        if !top_fragment.is_empty() {
            for close in ["</InLine>", "</Wrapper>"] {
                if let Some(updated) = insert_before(&out, close, &top_fragment) {
                    out = updated;
                    break;
                }
            }
        }

        // Insert tracking events before </Linear>, merging if present.
        if !tracking_block.is_empty() {
            if let Some(te_start) = out.find("<TrackingEvents>") {
                // Splice children into the existing block.
                let children = self.build_tracking_children(events, macros);
                let insert_at = te_start + "<TrackingEvents>".len();
                let mut merged = String::with_capacity(out.len() + children.len());
                merged.push_str(&out[..insert_at]);
                merged.push_str(&children);
                merged.push_str(&out[insert_at..]);
                out = merged;
            } else if let Some(updated) = insert_before(&out, "</Linear>", &tracking_block) {
                out = updated;
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_events() -> TrackingEvents {
        let mut e = TrackingEvents::new();
        e.impressions.push("http://imp.example/a?bid=##PBS-BIDID##".into());
        e.errors.push("http://err.example/e".into());
        let mut tev = HashMap::new();
        tev.insert("start".into(), vec!["http://t.example/start".into()]);
        tev.insert("complete".into(), vec!["http://t.example/complete".into()]);
        e.tracking_events = tev;
        e
    }

    #[test]
    fn injects_impression_and_error_before_inline_close() {
        let mut m = MacroMap::new();
        m.insert("PBS-BIDID".into(), "B1".into());
        let inj = VastInjector::new();
        let vast = r#"<VAST><Ad><InLine><AdSystem>x</AdSystem></InLine></Ad></VAST>"#;
        let out = inj.inject(vast, &make_events(), &m);
        assert!(out.contains("<Impression><![CDATA[http://imp.example/a?bid=B1]]></Impression>"));
        assert!(out.contains("<Error><![CDATA[http://err.example/e]]></Error>"));
        // Must sit before </InLine>.
        let imp_idx = out.find("<Impression>").unwrap();
        let close_idx = out.find("</InLine>").unwrap();
        assert!(imp_idx < close_idx);
    }

    #[test]
    fn injects_tracking_events_before_linear_close() {
        let m = MacroMap::new();
        let inj = VastInjector::new();
        let vast = r#"<VAST><Ad><InLine><Creatives><Creative><Linear><Duration>00:00:15</Duration></Linear></Creative></Creatives></InLine></Ad></VAST>"#;
        let out = inj.inject(vast, &make_events(), &m);
        assert!(out.contains("<TrackingEvents>"));
        assert!(out.contains("event=\"start\""));
        assert!(out.contains("event=\"complete\""));
        let te_idx = out.find("<TrackingEvents>").unwrap();
        let lin_close = out.find("</Linear>").unwrap();
        assert!(te_idx < lin_close);
    }

    #[test]
    fn merges_into_existing_tracking_events() {
        let m = MacroMap::new();
        let inj = VastInjector::new();
        let vast = r#"<VAST><Ad><InLine><Creatives><Creative><Linear><TrackingEvents><Tracking event="midpoint"><![CDATA[http://old]]></Tracking></TrackingEvents></Linear></Creative></Creatives></InLine></Ad></VAST>"#;
        let out = inj.inject(vast, &make_events(), &m);
        // Only one <TrackingEvents> wrapper.
        assert_eq!(out.matches("<TrackingEvents>").count(), 1);
        assert!(out.contains("event=\"midpoint\""));
        assert!(out.contains("event=\"start\""));
    }

    #[test]
    fn empty_markup_passthrough() {
        let inj = VastInjector::new();
        let out = inj.inject("", &make_events(), &MacroMap::new());
        assert_eq!(out, "");
    }

    #[test]
    fn no_events_returns_unchanged_shape() {
        let inj = VastInjector::new();
        let vast = "<VAST><Ad><InLine></InLine></Ad></VAST>";
        let out = inj.inject(vast, &TrackingEvents::new(), &MacroMap::new());
        assert_eq!(out, vast);
    }
}
