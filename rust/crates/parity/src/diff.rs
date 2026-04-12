//! Structural JSON diff engine.

use crate::options::DiffOptions;
use serde_json::Value;

/// Kind of structural difference found at a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffKind {
    /// Value exists in the right-hand side but not the left-hand side.
    Added,
    /// Value exists in the left-hand side but not the right-hand side.
    Removed,
    /// Both sides have a value of the same JSON type, but the values differ.
    Changed,
    /// Both sides have a value, but their JSON types differ.
    TypeChanged,
}

/// A single difference between two JSON values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffEntry {
    /// Dot-separated path from the document root (arrays use `[idx]` syntax).
    pub path: String,
    /// Kind of difference.
    pub kind: DiffKind,
}

/// Lightweight namespace wrapper that mirrors the Go `JsonDiff` type. The
/// actual diffing logic lives on [`diff`]; this struct exists so callers can
/// configure and re-run diffs with a stable handle.
#[derive(Debug, Clone, Default)]
pub struct JsonDiff {
    /// Options used for every call to [`JsonDiff::diff`].
    pub options: DiffOptions,
}

impl JsonDiff {
    /// Construct a new diff runner with the given options.
    pub fn new(options: DiffOptions) -> Self {
        Self { options }
    }

    /// Run the diff, returning all structural mismatches.
    pub fn diff(&self, a: &Value, b: &Value) -> Vec<DiffEntry> {
        diff(a, b, &self.options)
    }
}

/// Compute structural differences between `a` and `b`, honoring `opts`.
///
/// Mirrors the behavior of the Go diff engine: mismatches are reported with
/// JSONPath-like dotted paths (`root.child[0].field`), and entries matching
/// `ignore_paths`/`ignore_regex` are silently suppressed.
pub fn diff(a: &Value, b: &Value, opts: &DiffOptions) -> Vec<DiffEntry> {
    let mut out = Vec::new();
    diff_inner(a, b, "", opts, &mut out);
    out
}

fn diff_inner(a: &Value, b: &Value, path: &str, opts: &DiffOptions, out: &mut Vec<DiffEntry>) {
    if opts.is_ignored(path) {
        return;
    }

    // Handle `treat_missing_as_null`: a Null on either side is considered
    // equivalent to an absent field, so collapse to equal.
    if opts.treat_missing_as_null && (a.is_null() || b.is_null()) && a != b {
        if (a.is_null() && b.is_null()) || a.is_null() || b.is_null() {
            // Fall through into the normal type check below but allow
            // null/absent equivalence.
            if a.is_null() && b.is_null() {
                return;
            }
        }
    }

    match (a, b) {
        (Value::Null, Value::Null) => {}
        (Value::Bool(x), Value::Bool(y)) => {
            if x != y {
                out.push(DiffEntry { path: path.to_string(), kind: DiffKind::Changed });
            }
        }
        (Value::Number(x), Value::Number(y)) => {
            let fx = x.as_f64();
            let fy = y.as_f64();
            match (fx, fy) {
                (Some(fx), Some(fy)) => {
                    let delta = (fx - fy).abs();
                    if delta > opts.numeric_tolerance && fx != fy {
                        out.push(DiffEntry {
                            path: path.to_string(),
                            kind: DiffKind::Changed,
                        });
                    }
                }
                _ => {
                    if x != y {
                        out.push(DiffEntry {
                            path: path.to_string(),
                            kind: DiffKind::Changed,
                        });
                    }
                }
            }
        }
        (Value::String(x), Value::String(y)) => {
            if x != y {
                out.push(DiffEntry { path: path.to_string(), kind: DiffKind::Changed });
            }
        }
        (Value::Array(xa), Value::Array(ya)) => {
            let max = xa.len().max(ya.len());
            for i in 0..max {
                let child_path = if path.is_empty() {
                    format!("[{i}]")
                } else {
                    format!("{path}[{i}]")
                };
                if opts.is_ignored(&child_path) {
                    continue;
                }
                match (xa.get(i), ya.get(i)) {
                    (Some(xv), Some(yv)) => diff_inner(xv, yv, &child_path, opts, out),
                    (Some(_), None) => {
                        out.push(DiffEntry { path: child_path, kind: DiffKind::Removed });
                    }
                    (None, Some(_)) => {
                        out.push(DiffEntry { path: child_path, kind: DiffKind::Added });
                    }
                    (None, None) => {}
                }
            }
        }
        (Value::Object(xo), Value::Object(yo)) => {
            for (k, xv) in xo {
                let child_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                if opts.is_ignored(&child_path) {
                    continue;
                }
                match yo.get(k) {
                    Some(yv) => diff_inner(xv, yv, &child_path, opts, out),
                    None => {
                        if opts.treat_missing_as_null && xv.is_null() {
                            continue;
                        }
                        out.push(DiffEntry { path: child_path, kind: DiffKind::Removed });
                    }
                }
            }
            for (k, yv) in yo {
                if xo.contains_key(k) {
                    continue;
                }
                let child_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                if opts.is_ignored(&child_path) {
                    continue;
                }
                if opts.treat_missing_as_null && yv.is_null() {
                    continue;
                }
                out.push(DiffEntry { path: child_path, kind: DiffKind::Added });
            }
        }
        _ => {
            // Type mismatch (including Null vs non-Null when
            // treat_missing_as_null is disabled).
            out.push(DiffEntry { path: path.to_string(), kind: DiffKind::TypeChanged });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn added_field() {
        let a = json!({"x": 1});
        let b = json!({"x": 1, "y": 2});
        let entries = diff(&a, &b, &DiffOptions::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "y");
        assert_eq!(entries[0].kind, DiffKind::Added);
    }

    #[test]
    fn removed_field() {
        let a = json!({"x": 1, "y": 2});
        let b = json!({"x": 1});
        let entries = diff(&a, &b, &DiffOptions::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "y");
        assert_eq!(entries[0].kind, DiffKind::Removed);
    }

    #[test]
    fn changed_value() {
        let a = json!({"x": 1});
        let b = json!({"x": 2});
        let entries = diff(&a, &b, &DiffOptions::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "x");
        assert_eq!(entries[0].kind, DiffKind::Changed);
    }

    #[test]
    fn type_changed() {
        let a = json!({"x": 1});
        let b = json!({"x": "one"});
        let entries = diff(&a, &b, &DiffOptions::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "x");
        assert_eq!(entries[0].kind, DiffKind::TypeChanged);
    }

    #[test]
    fn numeric_tolerance_within() {
        let a = json!({"x": 1.0});
        let b = json!({"x": 1.0001});
        let opts = DiffOptions { numeric_tolerance: 0.001, ..Default::default() };
        let entries = diff(&a, &b, &opts);
        assert!(entries.is_empty(), "expected equal within tolerance, got {entries:?}");
    }

    #[test]
    fn numeric_tolerance_exceeded() {
        let a = json!({"x": 1.0});
        let b = json!({"x": 1.5});
        let opts = DiffOptions { numeric_tolerance: 0.001, ..Default::default() };
        let entries = diff(&a, &b, &opts);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, DiffKind::Changed);
    }

    #[test]
    fn ignore_top_level_path() {
        let a = json!({"x": 1, "y": 2});
        let b = json!({"x": 1, "y": 99});
        let opts = DiffOptions {
            ignore_paths: vec!["y".to_string()],
            ..Default::default()
        };
        let entries = diff(&a, &b, &opts);
        assert!(entries.is_empty(), "ignored path should be suppressed, got {entries:?}");
    }
}
