//! `Ext` object helpers (seller-rs `types/helpers.rs`, ext JSON section): key-order-preserving
//! insert/remove and path getters, mirroring the Go `AddToRawMessage` family.

use serde::Serialize;
use sonic_rs::{JsonContainerTrait, JsonValueTrait};

use crate::ortb::Ext;


// ── ext JSON ─────────────────────────────────────────────────────────────────────────────────

/// Set `key` in an `ext` object, keeping the key order (Go `AddToRawMessage` /
/// `AddBytesToRawMessage`).
///
/// An existing key keeps its position, a new one is appended, a missing ext becomes
/// `{key: value}` and a non-object ext is left unchanged, as with fastjson's `Set`.
///
/// A new key is spliced into the object's text before the closing `}` (OPT-31, Go's
/// `AddToRawMessage` splice): one write of the object and one parse, no per-entry work.
pub fn ext_insert<T: Serialize + ?Sized>(
    ext: &mut Option<Ext>,
    key: &str,
    value: &T,
) -> Result<(), sonic_rs::Error> {
    // Serialized straight to text: `to_value` would build a hash map and lose field order.
    let value = sonic_rs::to_string(value)?;
    let rebuilt = match ext.as_ref() {
        None => splice_into_object(b"{}".to_vec(), key, &value)?,
        Some(current) if current.0.get(key).is_some() => {
            rebuild_object(object_entries(current), key, Some(&value))?
        }
        Some(current) if current.0.is_object() => {
            splice_into_object(sonic_rs::to_vec(&current.0)?, key, &value)?
        }
        Some(_) => return Ok(()),
    };
    *ext = Some(Ext(rebuilt));
    Ok(())
}

/// Parses the compact object text `object` with `"key":value` appended.
fn splice_into_object(
    mut object: Vec<u8>,
    key: &str,
    value: &str,
) -> Result<sonic_rs::Value, sonic_rs::Error> {
    // Compact `sonic_rs` output: the object ends with `}`, and `{}` is the empty object.
    object.pop();
    if object.len() > 1 {
        object.push(b',');
    }
    sonic_rs::to_writer(&mut object, key)?;
    object.push(b':');
    object.extend_from_slice(value.as_bytes());
    object.push(b'}');
    sonic_rs::from_slice(&object)
}

/// Remove `key` from an `ext` object, keeping the other keys in order (Go
/// `RemoveFromRawMessage`).
pub fn ext_remove(ext: &mut Option<Ext>, key: &str) -> Result<(), sonic_rs::Error> {
    let Some(current) = ext.as_ref() else {
        return Ok(());
    };
    if current.0.get(key).is_none() {
        return Ok(());
    }
    let rebuilt = rebuild_object(object_entries(current), key, None)?;
    *ext = Some(Ext(rebuilt));
    Ok(())
}

fn object_entries(ext: &Ext) -> impl Iterator<Item = (&str, &sonic_rs::Value)> {
    ext.0.as_object().into_iter().flat_map(|obj| obj.iter())
}

/// Re-parse the object from text with `key` set to the JSON text `value` (in place, or
/// appended), or dropped when `value` is `None`.
///
/// Mutating a `sonic_rs` object directly turns it into a hash map and loses the key order; a
/// freshly parsed one keeps the source order, like Go's fastjson.
fn rebuild_object<'a>(
    entries: impl Iterator<Item = (&'a str, &'a sonic_rs::Value)>,
    key: &str,
    value: Option<&str>,
) -> Result<sonic_rs::Value, sonic_rs::Error> {
    fn push(json: &mut String, key: &str, value: &str) -> Result<(), sonic_rs::Error> {
        if json.len() > 1 {
            json.push(',');
        }
        json.push_str(&sonic_rs::to_string(key)?);
        json.push(':');
        json.push_str(value);
        Ok(())
    }

    let mut json = String::from("{");
    let mut pending = value;
    for (k, v) in entries {
        if k == key {
            if let Some(new) = pending.take() {
                push(&mut json, k, new)?;
            }
            continue;
        }
        push(&mut json, k, &sonic_rs::to_string(v)?)?;
    }
    if let Some(new) = pending {
        push(&mut json, key, new)?;
    }
    json.push('}');
    sonic_rs::from_str(&json)
}

/// The value at `path` (Go fastjson `Exists` + `Get`); `None` when any key is missing.
pub fn ext_get<'a>(ext: Option<&'a Ext>, path: &[&str]) -> Option<&'a sonic_rs::Value> {
    path.iter().try_fold(&ext?.0, |value, key| value.get(*key))
}

/// Integer at `path`: `Some(0)` when present but not an integer (Go `GetIntFromRawMessage`).
pub fn ext_i64(ext: Option<&Ext>, path: &[&str]) -> Option<i64> {
    ext_get(ext, path).map(|v| v.as_i64().unwrap_or(0))
}

/// Number at `path`: `Some(0.0)` when present but not a number (Go `GetFloatFromRawMessage`).
pub fn ext_f64(ext: Option<&Ext>, path: &[&str]) -> Option<f64> {
    ext_get(ext, path).map(|v| v.as_f64().unwrap_or(0.0))
}

/// Bool at `path`: `Some(false)` when present but not a bool (Go `GetBoolFromRawMessage`).
pub fn ext_bool(ext: Option<&Ext>, path: &[&str]) -> Option<bool> {
    ext_get(ext, path).map(|v| v.as_bool().unwrap_or(false))
}

/// String at `path`: `Some("")` when present but not a string
/// (Go `GetStringFromRawMessageNested`).
pub fn ext_str<'a>(ext: Option<&'a Ext>, path: &[&str]) -> Option<&'a str> {
    ext_get(ext, path).map(|v| v.as_str().unwrap_or_default())
}

/// String of the first of `keys` present at the top level, `""` when none is (Go
/// `GetFirstMatchingStringKey`).
pub fn ext_first_str<'a>(ext: Option<&'a Ext>, keys: &[&str]) -> &'a str {
    ext.and_then(|ext| keys.iter().find_map(|key| ext.0.get(*key)))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
}

