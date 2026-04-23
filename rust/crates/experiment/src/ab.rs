//! Simple A/B bucketing.
//!
//! Experiments typically want to put a deterministic subset of traffic into
//! a "variant" while the rest runs the default ("control") code path. The
//! most common primitive is to hash an identifier (account id, request id,
//! etc.) into a bucket `[0, 100)` and compare against a percentage weight.
//!
//! This module provides both the raw [`bucket_for`] function and a richer
//! [`ExperimentConfig`] + [`select_variant`] API that supports multi-way
//! splits.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A single experiment variant and its weight (percentage).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Variant {
    /// Human-readable name, e.g. `"control"`, `"treatment"`.
    pub name: String,
    /// Weight in percentage points. The sum of all weights in a config
    /// must be at most 100.
    pub weight: u32,
}

/// Configuration for a single experiment split.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ExperimentConfig {
    /// Unique experiment identifier. Used as hash salt so that the same
    /// account can be placed in different buckets for different experiments.
    pub id: String,
    /// Whether the experiment is active.
    #[serde(default)]
    pub enabled: bool,
    /// Mutually exclusive variants to select from.
    #[serde(default)]
    pub variants: Vec<Variant>,
}

/// Errors returned by [`select_variant`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AbError {
    /// The sum of all variant weights exceeded 100.
    #[error("variant weights total {0}, which exceeds 100")]
    WeightsExceed100(u32),
    /// The experiment has no variants defined.
    #[error("experiment '{0}' has no variants")]
    EmptyVariants(String),
}

/// Compute a deterministic bucket in `[0, 100)` for the given `(experiment, subject)`
/// pair.
///
/// The hashing function intentionally uses Rust's stable `DefaultHasher` so
/// the output is stable across runs of the same binary (but not guaranteed
/// across Rust versions). For a truly cross-language stable bucket you would
/// want e.g. SHA-256; this is sufficient for unit tests and for within-process
/// consistency in a single deployment.
pub fn bucket_for(experiment_id: &str, subject: &str) -> u32 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    experiment_id.hash(&mut hasher);
    0xabu8.hash(&mut hasher);
    subject.hash(&mut hasher);
    let h = hasher.finish();
    (h % 100) as u32
}

/// Select a variant for the given subject under the given experiment config.
///
/// If the experiment is disabled or the subject's bucket falls outside the
/// sum of variant weights, `Ok(None)` is returned and callers should fall
/// back to the default code path.
pub fn select_variant<'a>(
    cfg: &'a ExperimentConfig,
    subject: &str,
) -> Result<Option<&'a Variant>, AbError> {
    if !cfg.enabled {
        return Ok(None);
    }
    if cfg.variants.is_empty() {
        return Err(AbError::EmptyVariants(cfg.id.clone()));
    }

    let total: u32 = cfg.variants.iter().map(|v| v.weight).sum();
    if total > 100 {
        return Err(AbError::WeightsExceed100(total));
    }

    let bucket = bucket_for(&cfg.id, subject);
    let mut cumulative = 0u32;
    for v in &cfg.variants {
        cumulative += v.weight;
        if bucket < cumulative {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_is_in_range() {
        for subject in ["alice", "bob", "carol", "dave", "erin", "frank"] {
            let b = bucket_for("exp-1", subject);
            assert!(b < 100);
        }
    }

    #[test]
    fn bucket_is_deterministic() {
        let a = bucket_for("exp-1", "account-42");
        let b = bucket_for("exp-1", "account-42");
        assert_eq!(a, b);
    }

    #[test]
    fn bucket_varies_by_experiment() {
        // Not a guarantee for every input, but for a reasonable spread it
        // must vary on at least one subject we try.
        let subjects = ["account-1", "account-2", "account-3", "account-4", "req-a"];
        let mut any_differ = false;
        for s in subjects {
            if bucket_for("exp-1", s) != bucket_for("exp-2", s) {
                any_differ = true;
                break;
            }
        }
        assert!(any_differ);
    }

    #[test]
    fn disabled_experiment_selects_none() {
        let cfg = ExperimentConfig {
            id: "exp".into(),
            enabled: false,
            variants: vec![Variant {
                name: "treatment".into(),
                weight: 50,
            }],
        };
        assert!(select_variant(&cfg, "user").unwrap().is_none());
    }

    #[test]
    fn weights_exceed_100_errors() {
        let cfg = ExperimentConfig {
            id: "exp".into(),
            enabled: true,
            variants: vec![
                Variant {
                    name: "a".into(),
                    weight: 60,
                },
                Variant {
                    name: "b".into(),
                    weight: 60,
                },
            ],
        };
        assert!(matches!(
            select_variant(&cfg, "user"),
            Err(AbError::WeightsExceed100(120))
        ));
    }

    #[test]
    fn empty_variants_errors() {
        let cfg = ExperimentConfig {
            id: "exp".into(),
            enabled: true,
            variants: vec![],
        };
        assert!(matches!(
            select_variant(&cfg, "user"),
            Err(AbError::EmptyVariants(_))
        ));
    }

    #[test]
    fn full_allocation_always_selects_a_variant() {
        let cfg = ExperimentConfig {
            id: "full".into(),
            enabled: true,
            variants: vec![
                Variant {
                    name: "a".into(),
                    weight: 50,
                },
                Variant {
                    name: "b".into(),
                    weight: 50,
                },
            ],
        };
        for i in 0..1000 {
            let subject = format!("user-{i}");
            let v = select_variant(&cfg, &subject).unwrap();
            assert!(v.is_some());
        }
    }

    #[test]
    fn approximate_split_is_reasonable() {
        // Sanity check that the bucketing produces roughly 50/50 when
        // configured that way. Wide tolerance: 30%-70%.
        let cfg = ExperimentConfig {
            id: "split".into(),
            enabled: true,
            variants: vec![
                Variant {
                    name: "a".into(),
                    weight: 50,
                },
                Variant {
                    name: "b".into(),
                    weight: 50,
                },
            ],
        };
        let mut a_count = 0;
        let total = 2000;
        for i in 0..total {
            let subject = format!("u-{i}");
            let v = select_variant(&cfg, &subject).unwrap().unwrap();
            if v.name == "a" {
                a_count += 1;
            }
        }
        let ratio = a_count as f64 / total as f64;
        assert!(
            ratio > 0.30 && ratio < 0.70,
            "a-bucket ratio {ratio} outside 0.30..0.70"
        );
    }

    #[test]
    fn under_allocation_returns_none_for_some_subjects() {
        // Only 10% of traffic should hit the variant. Verify that at
        // least some subjects get None.
        let cfg = ExperimentConfig {
            id: "small".into(),
            enabled: true,
            variants: vec![Variant {
                name: "t".into(),
                weight: 10,
            }],
        };
        let mut none_seen = false;
        for i in 0..200 {
            let subject = format!("u-{i}");
            if select_variant(&cfg, &subject).unwrap().is_none() {
                none_seen = true;
                break;
            }
        }
        assert!(none_seen);
    }
}
