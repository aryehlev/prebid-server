//! Privacy Sandbox — Topics API support.
//! Mirrors Go `privacysandbox/topics.go`.
//!
//! Parses the `Sec-Browsing-Topics` HTTP header and merges topic segments
//! into OpenRTB `user.data`.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// A parsed topic segment from the Sec-Browsing-Topics header.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Topic {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segtax: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segclass: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segids: Option<Vec<i32>>,
}

/// Parse the Sec-Browsing-Topics header into a list of Topic objects.
/// Returns (topics, warnings). Mirrors Go `ParseTopicsFromHeader`.
pub fn parse_topics_from_header(sec_browsing_topics: &str) -> (Vec<Topic>, Vec<String>) {
    let mut topics = Vec::with_capacity(10);
    let mut warnings = Vec::new();

    for field in sec_browsing_topics.split(',') {
        let field = field.trim();
        if field.is_empty() || field.starts_with("();p=") {
            continue;
        }

        if topics.len() < 10 {
            if let Some(topic) = parse_topic_segment(field) {
                topics.push(topic);
            } else {
                warnings.push(format_warning(field));
            }
        } else {
            warnings.push(format_warning(&format!(
                "{} discarded due to limit reached.",
                field
            )));
        }
    }

    (topics, warnings)
}

/// Parse a single topic segment from the header.
fn parse_topic_segment(field: &str) -> Option<Topic> {
    let parts: Vec<&str> = field.splitn(2, ';').collect();
    if parts.len() != 2 {
        return None;
    }

    let segment_ids_str = parts[0].trim();
    if segment_ids_str.len() < 3
        || !segment_ids_str.starts_with('(')
        || !segment_ids_str.ends_with(')')
    {
        return None;
    }

    let (segtax, segclass) = parse_seg_tax_seg_class(parts[1]);
    if segtax == 0 || segclass.is_empty() {
        return None;
    }

    let inner = &segment_ids_str[1..segment_ids_str.len() - 1];
    let seg_ids = match parse_segment_ids(inner) {
        Ok(ids) => ids,
        Err(_) => return None,
    };

    Some(Topic {
        segtax: Some(segtax),
        segclass: Some(segclass),
        segids: Some(seg_ids),
    })
}

/// Parse the taxonomy version and segment class from the second part of a header field.
fn parse_seg_tax_seg_class(seg: &str) -> (i32, String) {
    let parts: Vec<&str> = seg.split(':').collect();
    if parts.len() != 3 {
        return (0, String::new());
    }

    // parts[0] is v=browser_version, we don't need it
    let taxonomy_ver = parts[1].trim();
    let taxonomy: i32 = match taxonomy_ver.parse() {
        Ok(v) if (1..=10).contains(&v) => v,
        _ => return (0, String::new()),
    };

    let segtax = 600 + (taxonomy - 1);
    let segclass = parts[2].trim().to_string();
    (segtax, segclass)
}

/// Parse segment IDs from the parenthesized portion of the header.
fn parse_segment_ids(segments_ids: &str) -> Result<Vec<i32>, String> {
    let mut result = Vec::new();
    for id_str in segments_ids.split_whitespace() {
        let id: i32 = id_str
            .trim()
            .parse()
            .map_err(|_| "invalid segment id".to_string())?;
        if id <= 0 {
            return Err("invalid segment id".to_string());
        }
        result.push(id);
    }
    Ok(result)
}

/// Type alias for the header data map: segtax -> segclass -> set of segment IDs.
type HeaderDataMap = HashMap<i32, HashMap<String, HashSet<i32>>>;

/// Update user.data with topics from the Sec-Browsing-Topics header.
/// Merges header-sourced topic segments into existing user data.
/// Mirrors Go `UpdateUserDataWithTopics`.
pub fn update_user_data_with_topics(
    user_data: &mut Vec<openrtb::Data>,
    header_data: &[Topic],
    topics_domain: &str,
) {
    if topics_domain.is_empty() {
        return;
    }

    let mut header_data_map = create_header_data_map(header_data);

    // Merge into existing user data entries
    for data in user_data.iter_mut() {
        let ext = match &data.ext {
            Some(ext) => ext,
            None => continue,
        };

        let topic: Topic = match serde_json::from_value(ext.clone()) {
            Ok(t) => t,
            Err(_) => continue,
        };

        let segtax = match topic.segtax {
            Some(st) if st != 0 => st,
            _ => continue,
        };
        let segclass = match &topic.segclass {
            Some(sc) if !sc.is_empty() => sc.clone(),
            _ => continue,
        };

        if let Some(new_seg_ids) =
            find_new_seg_ids(&data.name, topics_domain, segtax, &segclass, &data.segment, &header_data_map)
        {
            for seg_id in &new_seg_ids {
                data.segment.push(openrtb::Segment {
                    id: Some(seg_id.to_string()),
                    ..Default::default()
                });
            }

            if let Some(seg_class_map) = header_data_map.get_mut(&segtax) {
                seg_class_map.remove(&segclass);
            }
        }
    }

    // Add remaining header data as new user.data entries
    for (segtax, seg_class_map) in &header_data_map {
        for (segclass, seg_ids) in seg_class_map {
            if seg_ids.is_empty() {
                continue;
            }

            let ext_topic = Topic {
                segtax: Some(*segtax),
                segclass: Some(segclass.clone()),
                segids: None,
            };

            let ext = match serde_json::to_value(&ext_topic) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let mut segments = Vec::new();
            for seg_id in seg_ids {
                segments.push(openrtb::Segment {
                    id: Some(seg_id.to_string()),
                    ..Default::default()
                });
            }

            user_data.push(openrtb::Data {
                name: Some(topics_domain.to_string()),
                segment: segments,
                ext: Some(ext),
                ..Default::default()
            });
        }
    }
}

/// Create a lookup map from header data: segtax -> segclass -> set of segment IDs.
fn create_header_data_map(header_data: &[Topic]) -> HeaderDataMap {
    let mut map = HeaderDataMap::new();

    for topic in header_data {
        let segtax = topic.segtax.unwrap_or(0);
        let segclass = topic.segclass.clone().unwrap_or_default();
        let seg_ids = topic.segids.as_deref().unwrap_or_default();

        let seg_class_map = map.entry(segtax).or_default();
        let seg_ids_set = seg_class_map.entry(segclass).or_default();

        for &seg_id in seg_ids {
            seg_ids_set.insert(seg_id);
        }
    }

    map
}

/// Find new segment IDs to merge from header data into existing user data.
fn find_new_seg_ids(
    data_name: &Option<String>,
    topics_domain: &str,
    segtax: i32,
    segclass: &str,
    user_data_segments: &[openrtb::Segment],
    header_data_map: &HeaderDataMap,
) -> Option<Vec<i32>> {
    if data_name.as_deref() != Some(topics_domain) {
        return None;
    }

    let seg_class_map = header_data_map.get(&segtax)?;
    let seg_ids_map = seg_class_map.get(segclass)?;

    // Collect existing segment IDs
    let existing: HashSet<i32> = user_data_segments
        .iter()
        .filter_map(|s| s.id.as_ref()?.parse::<i32>().ok())
        .collect();

    // Return only new segment IDs
    let new_ids: Vec<i32> = seg_ids_map
        .iter()
        .filter(|id| !existing.contains(id))
        .copied()
        .collect();

    if new_ids.is_empty() {
        None
    } else {
        Some(new_ids)
    }
}

fn format_warning(msg: &str) -> String {
    format!("Invalid field in Sec-Browsing-Topics header: {}", msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_topics_empty() {
        let (topics, warnings) = parse_topics_from_header("");
        assert!(topics.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_topics_padding_only() {
        let (topics, warnings) = parse_topics_from_header("();p=P0000000000");
        assert!(topics.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_topics_valid_segment() {
        let (topics, warnings) =
            parse_topics_from_header("(1);v=chrome.1:1:2, ();p=P0000000000");
        assert_eq!(topics.len(), 1);
        assert!(warnings.is_empty());
        assert_eq!(topics[0].segtax, Some(600));
        assert_eq!(topics[0].segclass.as_deref(), Some("2"));
        assert_eq!(topics[0].segids.as_deref(), Some(&[1][..]));
    }

    #[test]
    fn test_parse_topics_multiple_segments() {
        let (topics, warnings) =
            parse_topics_from_header("(1 2);v=chrome.1:1:2, (3);v=chrome.1:2:3");
        assert_eq!(topics.len(), 2);
        assert!(warnings.is_empty());
        assert_eq!(topics[0].segids.as_deref(), Some(&[1, 2][..]));
        assert_eq!(topics[1].segtax, Some(601));
    }

    #[test]
    fn test_parse_topics_invalid_segment_produces_warning() {
        let (topics, warnings) = parse_topics_from_header("invalid_data");
        assert!(topics.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Invalid field"));
    }

    #[test]
    fn test_parse_topics_limit_10() {
        let mut parts = Vec::new();
        for i in 1..=12 {
            parts.push(format!("({});v=chrome.1:1:cls{}", i, i));
        }
        let header = parts.join(", ");
        let (topics, warnings) = parse_topics_from_header(&header);
        assert_eq!(topics.len(), 10);
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("discarded due to limit reached"));
    }

    #[test]
    fn test_parse_segment_ids_valid() {
        let ids = parse_segment_ids("1 2 3").unwrap();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_segment_ids_invalid() {
        assert!(parse_segment_ids("1 -1 3").is_err());
        assert!(parse_segment_ids("abc").is_err());
    }

    #[test]
    fn test_parse_seg_tax_seg_class() {
        let (segtax, segclass) = parse_seg_tax_seg_class("v=chrome.1:3:myclass");
        assert_eq!(segtax, 602);
        assert_eq!(segclass, "myclass");
    }

    #[test]
    fn test_parse_seg_tax_seg_class_invalid_taxonomy() {
        let (segtax, _) = parse_seg_tax_seg_class("v=chrome.1:0:myclass");
        assert_eq!(segtax, 0);

        let (segtax, _) = parse_seg_tax_seg_class("v=chrome.1:11:myclass");
        assert_eq!(segtax, 0);
    }

    #[test]
    fn test_update_user_data_empty_domain() {
        let mut user_data = Vec::new();
        let topics = vec![Topic {
            segtax: Some(600),
            segclass: Some("2".to_string()),
            segids: Some(vec![1]),
        }];
        update_user_data_with_topics(&mut user_data, &topics, "");
        assert!(user_data.is_empty());
    }

    #[test]
    fn test_update_user_data_adds_new_entries() {
        let mut user_data = Vec::new();
        let topics = vec![Topic {
            segtax: Some(600),
            segclass: Some("2".to_string()),
            segids: Some(vec![1, 2]),
        }];
        update_user_data_with_topics(&mut user_data, &topics, "chrome.com");
        assert_eq!(user_data.len(), 1);
        assert_eq!(user_data[0].name.as_deref(), Some("chrome.com"));
        assert_eq!(user_data[0].segment.len(), 2);
    }
}
