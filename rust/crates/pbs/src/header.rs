//! Go `net/http.Header`: canonical-cased keys, multiple values per key.
//!
//! Serializes as `{"Content-Type": ["application/json"]}`, which is what the fixtures compare
//! (`json.Marshal(actual.Headers)`), so a `BTreeMap` keeps the output deterministic.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Header(#[serde(deserialize_with = "null_as_empty")] BTreeMap<String, Vec<String>>);

/// Go decodes a JSON `null` header value into a nil slice, so it is an empty value list.
fn null_as_empty<'de, D>(d: D) -> Result<BTreeMap<String, Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<BTreeMap<String, Option<Vec<String>>>>::deserialize(d)?;
    Ok(raw
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, v.unwrap_or_default()))
        .collect())
}

impl Header {
    pub fn new() -> Self {
        Self::default()
    }

    /// Go `textproto.CanonicalMIMEHeaderKey`: first letter and letters after `-` upper-cased,
    /// the rest lower-cased. Keys with a space or other invalid byte are returned unchanged.
    pub fn canonical_key(key: &str) -> String {
        if key
            .bytes()
            .any(|b| !(b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)))
        {
            return key.to_string();
        }
        let mut out = String::with_capacity(key.len());
        let mut upper = true;
        for c in key.chars() {
            out.push(if upper { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() });
            upper = c == '-';
        }
        out
    }

    /// Go `h[key] = values` / a `http.Header{...}` literal: the key is stored exactly as given,
    /// without canonicalisation (`X-OpenRTB-Version` stays that way). `get`/`set` canonicalise,
    /// so they do not see such a key, as in Go.
    pub fn insert_raw(&mut self, key: &str, values: Vec<String>) {
        self.0.insert(key.to_string(), values);
    }

    /// Go `Header.Set`.
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.0.insert(Self::canonical_key(key), vec![value.into()]);
    }

    /// Go `Header.Add`.
    pub fn add(&mut self, key: &str, value: impl Into<String>) {
        self.0.entry(Self::canonical_key(key)).or_default().push(value.into());
    }

    /// Go `Header.Get`: first value, `""` when absent.
    pub fn get(&self, key: &str) -> &str {
        self.0
            .get(&Self::canonical_key(key))
            .and_then(|v| v.first())
            .map_or("", String::as_str)
    }

    /// Go `Header.Values`.
    pub fn values(&self, key: &str) -> &[String] {
        self.0.get(&Self::canonical_key(key)).map_or(&[], Vec::as_slice)
    }

    /// Go `Header.Del`.
    pub fn del(&mut self, key: &str) {
        self.0.remove(&Self::canonical_key(key));
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
