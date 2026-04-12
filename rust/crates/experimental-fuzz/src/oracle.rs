//! Differential oracles that compare an original JSON payload with a
//! mutated version to decide whether the mutation produced an observable,
//! expected, or silent change.

use serde_json::Value;

/// Result of running an [`Oracle`] against a pair of values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleResult {
    /// Values are semantically equivalent under this oracle.
    Same,
    /// Values differ; the `String` describes where / how.
    Different(String),
    /// The mutation was expected to produce a (tolerated) failure, e.g.
    /// dropping an optional field that a mock validator rejects gracefully.
    ExpectedFailure,
}

/// An oracle for checking invariants across mutations.
pub trait Oracle {
    fn check(&self, original: &Value, mutated: &Value) -> OracleResult;
}

/// Verifies that `serde_json` round-tripping preserves the structure of
/// both the original and mutated values, and that when they are equal the
/// round-trip is stable.
#[derive(Debug, Default, Clone)]
pub struct SerializationOracle;

impl Oracle for SerializationOracle {
    fn check(&self, original: &Value, mutated: &Value) -> OracleResult {
        // Round-trip both sides.
        let o_str = match serde_json::to_string(original) {
            Ok(s) => s,
            Err(e) => return OracleResult::Different(format!("original serialize failed: {e}")),
        };
        let m_str = match serde_json::to_string(mutated) {
            Ok(s) => s,
            Err(e) => return OracleResult::Different(format!("mutated serialize failed: {e}")),
        };
        let o_back: Value = match serde_json::from_str(&o_str) {
            Ok(v) => v,
            Err(e) => return OracleResult::Different(format!("original deserialize failed: {e}")),
        };
        let m_back: Value = match serde_json::from_str(&m_str) {
            Ok(v) => v,
            Err(e) => return OracleResult::Different(format!("mutated deserialize failed: {e}")),
        };

        if &o_back != original {
            return OracleResult::Different("original not stable under round-trip".to_string());
        }
        if &m_back != mutated {
            return OracleResult::Different("mutated not stable under round-trip".to_string());
        }
        if original == mutated {
            OracleResult::Same
        } else {
            OracleResult::Different("original != mutated (expected after mutation)".to_string())
        }
    }
}

/// Mock validator behavior used by [`FieldRemovalOracle`]. Considers a
/// request "valid" when it has an `id` string and a non-empty `imp`
/// array.
fn mock_validate(v: &Value) -> bool {
    v.get("id").and_then(|x| x.as_str()).is_some()
        && v.get("imp")
            .and_then(|x| x.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false)
}

/// Verifies that removing an *optional* field does not cause the mock
/// validator to panic / fail. Returns [`OracleResult::Same`] if both
/// original and mutated pass validation, [`OracleResult::ExpectedFailure`]
/// if removing the field caused the (tolerable) validation failure, and
/// [`OracleResult::Different`] if the mutation caused unrelated breakage.
#[derive(Debug, Clone)]
pub struct FieldRemovalOracle {
    /// Set of top-level field names considered "optional".
    pub optional_fields: Vec<String>,
}

impl FieldRemovalOracle {
    pub fn new(optional: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            optional_fields: optional.into_iter().map(Into::into).collect(),
        }
    }
}

impl Oracle for FieldRemovalOracle {
    fn check(&self, original: &Value, mutated: &Value) -> OracleResult {
        let orig_ok = mock_validate(original);
        let mut_ok = mock_validate(mutated);

        // Find which top-level keys were removed.
        let removed: Vec<String> = match (original.as_object(), mutated.as_object()) {
            (Some(o), Some(m)) => o.keys().filter(|k| !m.contains_key(*k)).cloned().collect(),
            _ => vec![],
        };

        if orig_ok && mut_ok {
            OracleResult::Same
        } else if orig_ok
            && !mut_ok
            && !removed.is_empty()
            && removed.iter().all(|k| self.optional_fields.contains(k))
        {
            OracleResult::ExpectedFailure
        } else if !orig_ok {
            OracleResult::Different("original failed validation".to_string())
        } else {
            OracleResult::Different(format!(
                "mutation broke validation unexpectedly; removed keys: {removed:?}"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serialization_oracle_detects_corruption() {
        // Build a value, then corrupt it so the mutated differs.
        let original = json!({ "id": "r1", "tmax": 500 });
        let mutated = json!({ "id": "r1", "tmax": -500 });
        let result = SerializationOracle.check(&original, &mutated);
        match result {
            OracleResult::Different(msg) => assert!(msg.contains("original != mutated")),
            other => panic!("expected Different, got {other:?}"),
        }
    }

    #[test]
    fn serialization_oracle_same_when_identical() {
        let v = json!({ "id": "r1" });
        assert_eq!(SerializationOracle.check(&v, &v), OracleResult::Same);
    }

    #[test]
    fn field_removal_oracle_tolerates_optional_drop() {
        let original = json!({ "id": "r1", "imp": [{"id": "i1"}], "tmax": 500 });
        let mutated = json!({ "id": "r1", "imp": [{"id": "i1"}] });
        let oracle = FieldRemovalOracle::new(["tmax"]);
        // Both still valid → Same.
        assert_eq!(oracle.check(&original, &mutated), OracleResult::Same);
    }

    #[test]
    fn field_removal_oracle_flags_required_drop() {
        let original = json!({ "id": "r1", "imp": [{"id": "i1"}] });
        let mutated = json!({ "imp": [{"id": "i1"}] });
        let oracle = FieldRemovalOracle::new(["tmax"]);
        match oracle.check(&original, &mutated) {
            OracleResult::Different(_) => {}
            other => panic!("expected Different, got {other:?}"),
        }
    }
}
