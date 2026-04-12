//! HTML tracker injection.
//!
//! Adds a tracking `<img>` pixel (and optionally a `<script>` tag) for
//! each impression URL. New elements are inserted immediately before
//! `</body>` if a body tag exists, otherwise appended to the end of the
//! markup.

use super::macros_support::{MacroMap, SimpleMacros};
use super::{TrackerInjector, TrackingEvents};

/// Controls whether the HTML injector emits `<img>` pixels or `<script>`
/// tags for impression URLs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HtmlMode {
    #[default]
    Img,
    Script,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HtmlInjector {
    pub mode: HtmlMode,
    macros: SimpleMacros,
}

impl HtmlInjector {
    pub fn new() -> Self {
        Self {
            mode: HtmlMode::Img,
            macros: SimpleMacros::new(),
        }
    }

    pub fn with_mode(mode: HtmlMode) -> Self {
        Self {
            mode,
            macros: SimpleMacros::new(),
        }
    }

    fn render_tag(&self, url: &str) -> String {
        match self.mode {
            HtmlMode::Img => format!(
                "<img src=\"{}\" width=\"1\" height=\"1\" style=\"display:none\" alt=\"\"/>",
                escape_attr(url)
            ),
            HtmlMode::Script => format!(
                "<script src=\"{}\" async></script>",
                escape_attr(url)
            ),
        }
    }
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;")
}

impl TrackerInjector for HtmlInjector {
    fn inject(&self, markup: &str, events: &TrackingEvents, macros: &MacroMap) -> String {
        if events.impressions.is_empty() {
            return markup.to_string();
        }

        let mut fragment = String::new();
        for url in &events.impressions {
            let resolved = self.macros.resolve(url, macros);
            fragment.push_str(&self.render_tag(&resolved));
        }

        if let Some(idx) = markup.find("</body>") {
            let mut out = String::with_capacity(markup.len() + fragment.len());
            out.push_str(&markup[..idx]);
            out.push_str(&fragment);
            out.push_str(&markup[idx..]);
            out
        } else {
            let mut out = String::with_capacity(markup.len() + fragment.len());
            out.push_str(markup);
            out.push_str(&fragment);
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events() -> TrackingEvents {
        let mut e = TrackingEvents::new();
        e.impressions.push("http://px.example/?x=1&y=##PBS-BIDID##".into());
        e
    }

    #[test]
    fn injects_img_before_body_close() {
        let inj = HtmlInjector::new();
        let mut m = MacroMap::new();
        m.insert("PBS-BIDID".into(), "B1".into());
        let html = "<html><body><p>hi</p></body></html>";
        let out = inj.inject(html, &events(), &m);
        assert!(out.contains("<img src=\"http://px.example/?x=1&amp;y=B1\""));
        let img_idx = out.find("<img").unwrap();
        let body_close = out.find("</body>").unwrap();
        assert!(img_idx < body_close);
    }

    #[test]
    fn injects_script_when_requested() {
        let inj = HtmlInjector::with_mode(HtmlMode::Script);
        let m = MacroMap::new();
        let html = "<html><body></body></html>";
        let out = inj.inject(html, &events(), &m);
        assert!(out.contains("<script src=\""));
        assert!(out.contains("async></script>"));
    }

    #[test]
    fn appends_when_no_body() {
        let inj = HtmlInjector::new();
        let m = MacroMap::new();
        let html = "<div>hi</div>";
        let out = inj.inject(html, &events(), &m);
        assert!(out.starts_with("<div>hi</div>"));
        assert!(out.contains("<img src=\""));
    }

    #[test]
    fn passthrough_when_no_impressions() {
        let inj = HtmlInjector::new();
        let out = inj.inject("<html></html>", &TrackingEvents::new(), &MacroMap::new());
        assert_eq!(out, "<html></html>");
    }
}
