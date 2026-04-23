//! In-place JSON normalization prior to diffing.

use crate::options::DiffOptions;
use serde_json::Value;

/// Normalize `v` in place according to `opts`:
///
/// - Strips fields whose path matches `opts.ignore_paths`.
/// - When `opts.ignore_array_order` is set, sorts arrays by their canonical
///   string form so that order-insensitive comparisons are possible.
pub fn normalize(v: &mut Value, opts: &DiffOptions) {
    normalize_inner(v, "", opts);
}

fn normalize_inner(v: &mut Value, path: &str, opts: &DiffOptions) {
    match v {
        Value::Array(arr) => {
            // Strip ignored indices first, recurse, then optionally sort.
            for (i, child) in arr.iter_mut().enumerate() {
                let child_path = if path.is_empty() {
                    format!("[{i}]")
                } else {
                    format!("{path}[{i}]")
                };
                normalize_inner(child, &child_path, opts);
            }
            if opts.ignore_array_order {
                arr.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
            }
        }
        Value::Object(map) => {
            // Collect keys to drop based on ignore_paths.
            let to_drop: Vec<String> = map
                .keys()
                .filter(|k| {
                    let p = if path.is_empty() {
                        (*k).clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    opts.is_ignored(&p)
                })
                .cloned()
                .collect();
            for k in to_drop {
                map.remove(&k);
            }
            for (k, child) in map.iter_mut() {
                let child_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                normalize_inner(child, &child_path, opts);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_array_order() {
        let mut a = json!({"xs": [3, 1, 2]});
        let opts = DiffOptions { ignore_array_order: true, ..Default::default() };
        normalize(&mut a, &opts);
        assert_eq!(a, json!({"xs": [1, 2, 3]}));
    }

    #[test]
    fn normalize_strips_ignored_paths() {
        let mut a = json!({"keep": 1, "drop": 99, "nested": {"drop_me": true, "keep_me": 2}});
        let opts = DiffOptions {
            ignore_paths: vec!["drop".to_string(), "nested.drop_me".to_string()],
            ..Default::default()
        };
        normalize(&mut a, &opts);
        assert_eq!(a, json!({"keep": 1, "nested": {"keep_me": 2}}));
    }
}
