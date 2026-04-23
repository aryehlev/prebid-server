//! Embedded scenario fixtures.
//!
//! The actual YAML lives under `tests/fixtures/` so the files are also usable
//! from integration tests on disk. We pull them into the binary via
//! [`include_str!`] so library consumers can use them without needing to
//! locate the crate directory on disk.

/// Contents of `tests/fixtures/simple_banner.yaml`.
pub const SIMPLE_BANNER_YAML: &str =
    include_str!("../../tests/fixtures/simple_banner.yaml");

/// Contents of `tests/fixtures/no_bid.yaml`.
pub const NO_BID_YAML: &str = include_str!("../../tests/fixtures/no_bid.yaml");

/// Every embedded scenario as `(name, body)` pairs.
pub fn all() -> &'static [(&'static str, &'static str)] {
    &[
        ("simple_banner", SIMPLE_BANNER_YAML),
        ("no_bid", NO_BID_YAML),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::Scenario;

    #[test]
    fn embedded_fixtures_parse() {
        for (name, body) in all() {
            let s = Scenario::load_from_yaml(body)
                .unwrap_or_else(|e| panic!("fixture {name} failed to parse: {e}"));
            assert!(!s.name.is_empty(), "fixture {name} has empty name");
        }
    }
}
