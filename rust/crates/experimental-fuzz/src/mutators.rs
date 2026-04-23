//! Mutation primitives for fuzzing existing JSON payloads.

use serde_json::Value;

/// A mutation strategy applied in place to a `serde_json::Value`.
pub trait Mutator {
    /// Apply the mutation to `value`. `seed` may influence randomized
    /// sub-behavior (unused by current simple mutators but part of the
    /// contract for future extensions).
    fn mutate(&self, value: &mut Value, seed: u64);
}

/// Drops (removes) the field located at a dotted JSON path. Array indices
/// may appear as bare integers (e.g. `imp.0.banner`). Missing paths are
/// silently ignored.
#[derive(Debug, Clone)]
pub struct DropField(pub String);

impl Mutator for DropField {
    fn mutate(&self, value: &mut Value, _seed: u64) {
        drop_at_path(value, &self.0);
    }
}

/// Negate a numeric field in place. Non-numeric or missing paths are
/// ignored.
#[derive(Debug, Clone)]
pub struct NegateNumber(pub String);

impl Mutator for NegateNumber {
    fn mutate(&self, value: &mut Value, _seed: u64) {
        if let Some(node) = get_mut_at_path(value, &self.0) {
            if let Some(n) = node.as_i64() {
                *node = Value::from(-n);
            } else if let Some(n) = node.as_f64() {
                *node = serde_json::Number::from_f64(-n)
                    .map(Value::Number)
                    .unwrap_or(Value::Null);
            }
        }
    }
}

/// Replace a string at the given path with a numeric value.
#[derive(Debug, Clone)]
pub struct SwapStringType(pub String);

impl Mutator for SwapStringType {
    fn mutate(&self, value: &mut Value, seed: u64) {
        if let Some(node) = get_mut_at_path(value, &self.0) {
            if node.is_string() {
                *node = Value::from((seed as i64).wrapping_add(42));
            }
        }
    }
}

/// Truncate an array at the given path to zero length.
#[derive(Debug, Clone)]
pub struct TruncateArray(pub String);

impl Mutator for TruncateArray {
    fn mutate(&self, value: &mut Value, _seed: u64) {
        if let Some(node) = get_mut_at_path(value, &self.0) {
            if let Some(arr) = node.as_array_mut() {
                arr.clear();
            }
        }
    }
}

/// Run a series of mutators against the same value, in order.
pub struct ChainMutator(pub Vec<Box<dyn Mutator>>);

impl Mutator for ChainMutator {
    fn mutate(&self, value: &mut Value, seed: u64) {
        for (i, m) in self.0.iter().enumerate() {
            m.mutate(value, seed.wrapping_add(i as u64));
        }
    }
}

// --- path helpers -----------------------------------------------------------

fn split_path(path: &str) -> Vec<&str> {
    path.split('.').filter(|s| !s.is_empty()).collect()
}

/// Walk the given dotted path and return a mutable reference to the
/// terminal node, or `None` if any segment is missing / typed wrong.
fn get_mut_at_path<'a>(root: &'a mut Value, path: &str) -> Option<&'a mut Value> {
    let segments = split_path(path);
    let mut cur = root;
    for seg in segments {
        cur = step_mut(cur, seg)?;
    }
    Some(cur)
}

fn step_mut<'a>(node: &'a mut Value, seg: &str) -> Option<&'a mut Value> {
    match node {
        Value::Object(map) => map.get_mut(seg),
        Value::Array(arr) => seg.parse::<usize>().ok().and_then(move |i| arr.get_mut(i)),
        _ => None,
    }
}

/// Drop the entry located at the given path. Handles object keys and
/// array indices.
fn drop_at_path(root: &mut Value, path: &str) {
    let segments = split_path(path);
    if segments.is_empty() {
        return;
    }
    let (last, parents) = segments.split_last().unwrap();

    let mut cur = root;
    for seg in parents {
        match step_mut(cur, seg) {
            Some(next) => cur = next,
            None => return,
        }
    }

    match cur {
        Value::Object(map) => {
            map.remove(*last);
        }
        Value::Array(arr) => {
            if let Ok(i) = last.parse::<usize>() {
                if i < arr.len() {
                    arr.remove(i);
                }
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
    fn drop_field_removes_top_level_key() {
        let mut v = json!({ "a": 1, "b": 2 });
        DropField("a".into()).mutate(&mut v, 0);
        assert!(v.get("a").is_none());
        assert_eq!(v.get("b"), Some(&json!(2)));
    }

    #[test]
    fn drop_field_removes_nested_key() {
        let mut v = json!({ "outer": { "inner": 7 } });
        DropField("outer.inner".into()).mutate(&mut v, 0);
        assert!(v["outer"].get("inner").is_none());
    }

    #[test]
    fn negate_number_flips_sign() {
        let mut v = json!({ "tmax": 1000 });
        NegateNumber("tmax".into()).mutate(&mut v, 0);
        assert_eq!(v["tmax"], json!(-1000));
    }

    #[test]
    fn negate_number_handles_floats() {
        let mut v = json!({ "bid": 1.5 });
        NegateNumber("bid".into()).mutate(&mut v, 0);
        assert_eq!(v["bid"].as_f64(), Some(-1.5));
    }

    #[test]
    fn swap_string_type_replaces_string_with_number() {
        let mut v = json!({ "id": "req-1" });
        SwapStringType("id".into()).mutate(&mut v, 0);
        assert!(v["id"].is_number());
    }

    #[test]
    fn truncate_array_clears() {
        let mut v = json!({ "imp": [1, 2, 3] });
        TruncateArray("imp".into()).mutate(&mut v, 0);
        assert_eq!(v["imp"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn chain_mutator_applies_all() {
        let mut v = json!({ "id": "x", "tmax": 500, "imp": [1, 2] });
        let chain = ChainMutator(vec![
            Box::new(DropField("id".into())),
            Box::new(NegateNumber("tmax".into())),
            Box::new(TruncateArray("imp".into())),
        ]);
        chain.mutate(&mut v, 0);
        assert!(v.get("id").is_none());
        assert_eq!(v["tmax"], json!(-500));
        assert_eq!(v["imp"].as_array().unwrap().len(), 0);
    }
}
