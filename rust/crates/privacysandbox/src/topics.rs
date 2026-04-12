//! Parser for the Chrome "Topics API" `Sec-Browsing-Topics` request header.
//!
//! Ported from `privacysandbox/topics.go`.
//!
//! The header value is a comma-separated list of entries shaped like
//! `(<space-separated ids>);v=chrome.<browser_version>:<taxonomy>:<classifier>`.
//! A trailing `();p=P0000...` padding entry may also appear and is ignored.

use serde::{Deserialize, Serialize};

/// A single Topics API entry, as produced by the browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topic {
    /// Segment taxonomy id. Prebid uses `600 + (taxonomy - 1)`.
    #[serde(rename = "segtax", skip_serializing_if = "is_zero", default)]
    pub seg_tax: i32,
    /// Classifier / model version from the browser.
    #[serde(rename = "segclass", skip_serializing_if = "String::is_empty", default)]
    pub seg_class: String,
    /// List of selected segment ids.
    #[serde(rename = "segids", skip_serializing_if = "Vec::is_empty", default)]
    pub seg_ids: Vec<i32>,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

/// A warning produced while parsing an individual field of the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseWarning {
    pub message: String,
}

impl ParseWarning {
    fn new(field: impl AsRef<str>) -> Self {
        ParseWarning {
            message: format!(
                "Invalid field in Sec-Browsing-Topics header: {}",
                field.as_ref()
            ),
        }
    }
}

impl std::fmt::Display for ParseWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ParseWarning {}

/// The result of parsing a `Sec-Browsing-Topics` header.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TopicsParseResult {
    pub topics: Vec<Topic>,
    pub warnings: Vec<ParseWarning>,
}

/// The maximum number of topic entries that will be returned by
/// `parse_topics_from_header`. Matches Go's hard-coded limit of 10.
pub const MAX_TOPICS: usize = 10;

/// Parses the raw `Sec-Browsing-Topics` header value into a set of topics
/// and a list of non-fatal warnings.
pub fn parse_topics_from_header(sec_browsing_topics: &str) -> TopicsParseResult {
    let mut topics: Vec<Topic> = Vec::with_capacity(MAX_TOPICS);
    let mut warnings: Vec<ParseWarning> = Vec::new();

    for field in sec_browsing_topics.split(',') {
        let field = field.trim();
        if field.is_empty() || field.starts_with("();p=") {
            continue;
        }

        if topics.len() < MAX_TOPICS {
            match parse_topic_segment(field) {
                Some(t) => topics.push(t),
                None => warnings.push(ParseWarning::new(field)),
            }
        } else {
            warnings.push(ParseWarning::new(format!(
                "{} discarded due to limit reached.",
                field
            )));
        }
    }

    TopicsParseResult { topics, warnings }
}

/// Parses a single `(<ids>);v=chrome.x:taxonomy:classifier` field.
fn parse_topic_segment(field: &str) -> Option<Topic> {
    let mut parts = field.splitn(2, ';');
    let ids_part = parts.next()?.trim();
    let meta_part = parts.next()?.trim();
    if parts.next().is_some() {
        // more than two ';' separated parts is invalid per Go parser
        return None;
    }

    if ids_part.len() < 3
        || !ids_part.starts_with('(')
        || !ids_part.ends_with(')')
    {
        return None;
    }

    let (seg_tax, seg_class) = parse_seg_tax_seg_class(meta_part)?;
    if seg_tax == 0 || seg_class.is_empty() {
        return None;
    }

    let inner = &ids_part[1..ids_part.len() - 1];
    let seg_ids = parse_segment_ids(inner)?;

    Some(Topic {
        seg_tax,
        seg_class,
        seg_ids,
    })
}

/// Parses the `v=chrome.x:taxonomy:classifier` metadata block.
///
/// The browser version part is ignored, the taxonomy part is validated to
/// be within `1..=10` and the segtax is computed as `600 + (taxonomy - 1)`.
fn parse_seg_tax_seg_class(seg: &str) -> Option<(i32, String)> {
    // The Go code splits on ':' and requires exactly 3 parts. Using the
    // same logic ensures byte-for-byte parity.
    let parts: Vec<&str> = seg.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    // parts[0] is "v=<browser>" - ignored.
    let taxonomy: i32 = parts[1].trim().parse().ok()?;
    if !(1..=10).contains(&taxonomy) {
        return None;
    }

    let seg_tax = 600 + (taxonomy - 1);
    let seg_class = parts[2].trim().to_string();

    Some((seg_tax, seg_class))
}

/// Parses a whitespace-separated list of positive integer segment ids.
fn parse_segment_ids(ids: &str) -> Option<Vec<i32>> {
    let mut out = Vec::new();
    for tok in ids.split_whitespace() {
        let n: i32 = tok.parse().ok()?;
        if n <= 0 {
            return None;
        }
        out.push(n);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_header() {
        let res = parse_topics_from_header(" \t ");
        assert!(res.topics.is_empty());
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn only_padding() {
        let res = parse_topics_from_header("();p=P0000000000000000000000000000000");
        assert!(res.topics.is_empty());
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn invalid_header_value() {
        let res = parse_topics_from_header("some-sec-cookie-value");
        assert!(res.topics.is_empty());
        assert_eq!(res.warnings.len(), 1);
        assert!(res.warnings[0]
            .message
            .contains("Invalid field in Sec-Browsing-Topics header: some-sec-cookie-value"));
    }

    #[test]
    fn single_valid_field_with_padding() {
        let res = parse_topics_from_header("(1);v=chrome.1:1:2, ();p=P00000000000");
        assert_eq!(
            res.topics,
            vec![Topic {
                seg_tax: 600,
                seg_class: "2".to_string(),
                seg_ids: vec![1],
            }]
        );
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn single_valid_field_without_padding() {
        let res = parse_topics_from_header("(1);v=chrome.1:1:2");
        assert_eq!(
            res.topics,
            vec![Topic {
                seg_tax: 600,
                seg_class: "2".to_string(),
                seg_ids: vec![1],
            }]
        );
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn multiple_valid_segment_ids() {
        let res = parse_topics_from_header("(1 2 3);v=chrome.1:1:2");
        assert_eq!(res.topics.len(), 1);
        assert_eq!(res.topics[0].seg_ids, vec![1, 2, 3]);
        assert_eq!(res.topics[0].seg_tax, 600);
        assert_eq!(res.topics[0].seg_class, "2");
    }

    #[test]
    fn taxonomy_offset_rules() {
        // taxonomy 1 => segtax 600
        // taxonomy 2 => segtax 601
        // taxonomy 10 => segtax 609
        let res = parse_topics_from_header(
            "(1);v=chrome.1:1:a, (2);v=chrome.1:2:b, (3);v=chrome.1:10:c",
        );
        assert_eq!(res.topics.len(), 3);
        assert_eq!(res.topics[0].seg_tax, 600);
        assert_eq!(res.topics[1].seg_tax, 601);
        assert_eq!(res.topics[2].seg_tax, 609);
    }

    #[test]
    fn taxonomy_out_of_range_is_warning() {
        let res = parse_topics_from_header("(1);v=chrome.1:11:a");
        assert!(res.topics.is_empty());
        assert_eq!(res.warnings.len(), 1);
    }

    #[test]
    fn more_than_ten_entries_are_truncated() {
        let header = "(1);v=chrome.1:1:2, (2);v=chrome.1:1:2, (3);v=chrome.1:1:2, \
                      (4);v=chrome.1:1:2, (5);v=chrome.1:1:2, (6);v=chrome.1:1:2, \
                      (7);v=chrome.1:1:2, (8);v=chrome.1:1:2, (9);v=chrome.1:1:2, \
                      (10);v=chrome.1:1:2, (11);v=chrome.1:1:2, (12);v=chrome.1:1:2, \
                      ();p=P00000000000";
        let res = parse_topics_from_header(header);
        assert_eq!(res.topics.len(), 10);
        // The 11th and 12th entries become "discarded due to limit reached" warnings.
        assert_eq!(res.warnings.len(), 2);
        for w in &res.warnings {
            assert!(w.message.contains("discarded due to limit reached."));
        }
    }

    #[test]
    fn invalid_segment_id_is_warning() {
        let res = parse_topics_from_header("(0);v=chrome.1:1:2");
        assert!(res.topics.is_empty());
        assert_eq!(res.warnings.len(), 1);

        let res = parse_topics_from_header("(abc);v=chrome.1:1:2");
        assert!(res.topics.is_empty());
        assert_eq!(res.warnings.len(), 1);
    }

    #[test]
    fn missing_parentheses_is_warning() {
        let res = parse_topics_from_header("1;v=chrome.1:1:2");
        assert!(res.topics.is_empty());
        assert_eq!(res.warnings.len(), 1);
    }

    #[test]
    fn topic_json_roundtrip() {
        let t = Topic {
            seg_tax: 600,
            seg_class: "2".into(),
            seg_ids: vec![1, 2, 3],
        };
        let s = serde_json::to_string(&t).unwrap();
        let back: Topic = serde_json::from_str(&s).unwrap();
        assert_eq!(t, back);
    }
}
