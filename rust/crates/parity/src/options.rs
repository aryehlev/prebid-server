//! Options controlling diffing and normalization.

use regex::Regex;

/// Knobs that drive [`crate::diff::diff`] and [`crate::normalize::normalize`].
#[derive(Debug, Default, Clone)]
pub struct DiffOptions {
    /// Exact JSON paths (dot-separated, e.g. `"ext.warnings"`) to drop from
    /// both inputs before diffing.
    pub ignore_paths: Vec<String>,
    /// Regexes applied to the full path of each candidate mismatch; any match
    /// causes the entry to be suppressed.
    pub ignore_regex: Vec<Regex>,
    /// Absolute tolerance used when comparing numeric values. `0.0` means
    /// strict equality.
    pub numeric_tolerance: f64,
    /// When set, arrays are sorted (by stringified form) prior to comparison.
    pub ignore_array_order: bool,
    /// When set, a missing field is treated as equivalent to an explicit
    /// `null`.
    pub treat_missing_as_null: bool,
}

impl DiffOptions {
    /// Convenience constructor that returns the default (strict) options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if `path` matches any configured ignore path or regex.
    pub fn is_ignored(&self, path: &str) -> bool {
        if self.ignore_paths.iter().any(|p| path == p || path.starts_with(&format!("{p}."))) {
            return true;
        }
        self.ignore_regex.iter().any(|re| re.is_match(path))
    }
}
