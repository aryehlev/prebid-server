//! [`Ext`]: the Rust stand-in for Go's `json.RawMessage` fields (`ext`, `requestobj`, `customdata`).

use std::borrow::Cow;
use std::fmt;
use std::ops::{Deref, DerefMut};

use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Error as _, Serializer};
use serde::{Deserialize, Serialize};
use sonic_rs::Value;

/// Arbitrary JSON carried through untouched: a `sonic_rs::Value`.
///
/// Parsed values keep the key order of the input, like Go's `RawMessage`. Inserting into an
/// object turns it into a hash map, so edited objects may be written in a different key order.
/// To add or remove a key while keeping the order, use
/// [`ext_insert`](crate::ext_helpers::ext_insert) / [`ext_remove`](crate::ext_helpers::ext_remove).
///
/// - JSON (any human-readable format): serialized and parsed as the JSON value itself, so it
///   works with `sonic_rs` and `serde_json`.
/// - msgpack (any non-human-readable format, i.e. `rmp_serde`): written as a `bin` holding the
///   compact JSON text and read back from `bin` or `str`. That is how Go's `vmihailenco/msgpack`
///   encodes a `json.RawMessage`, which keeps the bid-cache `DemandBid` blob compatible.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ext(pub Value);

impl Ext {
    /// Parses raw JSON text (Go `json.RawMessage(bytes)`).
    pub fn from_slice(json: &[u8]) -> sonic_rs::Result<Self> {
        sonic_rs::from_slice(json).map(Self)
    }

    /// Builds an `Ext` from any serializable value (Go `json.Marshal` into an ext field).
    pub fn from_serialize<T: Serialize + ?Sized>(value: &T) -> sonic_rs::Result<Self> {
        sonic_rs::to_value(value).map(Self)
    }

    /// Decodes the ext into a typed struct (Go `json.Unmarshal(ext, &target)`).
    ///
    /// Struct keys are matched ignoring case and an array is not read as a struct, as Go's
    /// decoder does (see [`crate::casefold`]). The decode runs over the JSON text, which keeps the
    /// key order, so the first bad field is the one Go would report.
    pub fn decode<T: serde::de::DeserializeOwned>(&self) -> Result<T, DecodeError> {
        let text = self.0.to_string();
        let run = |text: &str| {
            let mut de = serde_json::Deserializer::from_str(text);
            T::deserialize(crate::casefold::Fold(&mut de))
        };
        // A literal `null` is valid for an object target in Go and leaves the zero value; serde
        // rejects `null` for a struct. Read it as `{}`, the zero value of a struct whose fields
        // default (the same rule as `jsonutil::unmarshal`). A struct with a required field keeps
        // the original error.
        if text == "null" {
            if let Ok(v) = run("{}") {
                return Ok(v);
            }
        }
        run(&text).map_err(|e| DecodeError(strip_position(&e.to_string())))
    }

    /// Compact JSON text.
    pub fn to_json(&self) -> String {
        self.0.to_string()
    }

    pub fn into_inner(self) -> Value {
        self.0
    }
}

/// Why an [`Ext::decode`] failed. Its text has no `at line N column M` suffix (serde_json adds
/// one to every message, Go's decoder never does), so adapters can put it in user-facing errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError(String);

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DecodeError {}

/// Drops serde_json's trailing ` at line L column C`.
fn strip_position(msg: &str) -> String {
    match msg.rfind(" at line ") {
        Some(i) if msg[i..].contains(" column ") => msg[..i].to_string(),
        _ => msg.to_string(),
    }
}

impl From<Value> for Ext {
    fn from(value: Value) -> Self {
        Self(value)
    }
}

impl Deref for Ext {
    type Target = Value;

    fn deref(&self) -> &Value {
        &self.0
    }
}

impl DerefMut for Ext {
    fn deref_mut(&mut self) -> &mut Value {
        &mut self.0
    }
}

impl fmt::Display for Ext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl Serialize for Ext {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            // Go wrote a `json.RawMessage` as is: no HTML escaping inside it.
            let _raw = crate::go_json::RawMessageScope::enter();
            self.0.serialize(serializer)
        } else {
            let raw = sonic_rs::to_vec(&self.0).map_err(S::Error::custom)?;
            serializer.serialize_bytes(&raw)
        }
    }
}

impl<'de> Deserialize<'de> for Ext {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            // sonic_rs answers this newtype name by handing over the raw JSON text of the value
            // (the hook behind `sonic_rs::LazyValue`), which then parses into an order-preserving
            // `Value`. Every other deserializer treats it as a plain newtype; those are transcoded.
            deserializer
                .deserialize_newtype_struct(SONIC_RAW_TOKEN, JsonVisitor)
                .map(Self)
        } else {
            deserializer.deserialize_any(RawBytesVisitor).map(Self)
        }
    }
}

/// Newtype name `sonic_rs` uses for `LazyValue`. Guarded by `sonic_path_preserves_raw_order`.
const SONIC_RAW_TOKEN: &str = "$sonic_rs::LazyValue";

struct JsonVisitor;

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any JSON value")
    }

    /// sonic_rs: the raw JSON text of the value.
    fn visit_str<E: de::Error>(self, raw: &str) -> Result<Value, E> {
        sonic_rs::from_str(raw).map_err(E::custom)
    }

    /// Any other deserializer: re-encode the value as JSON text, then parse it with sonic_rs.
    /// (A `Value` built by inserting keys would not keep their order.)
    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Value, D::Error> {
        let mut json = Vec::with_capacity(64);
        Transcode(&mut json).deserialize(deserializer)?;
        sonic_rs::from_slice(&json).map_err(de::Error::custom)
    }
}

/// Writes whatever value the deserializer yields as compact JSON text.
struct Transcode<'a>(&'a mut Vec<u8>);

impl Transcode<'_> {
    fn write<T: Serialize + ?Sized, E: de::Error>(self, value: &T) -> Result<(), E> {
        sonic_rs::to_writer(&mut *self.0, value).map_err(E::custom)
    }
}

impl<'de> DeserializeSeed<'de> for Transcode<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Transcode<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<(), E> {
        self.write(&v)
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<(), E> {
        self.write(&v)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<(), E> {
        self.write(&v)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<(), E> {
        self.write(&v)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<(), E> {
        self.write(v)
    }

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        self.0.extend_from_slice(b"null");
        Ok(())
    }

    fn visit_none<E: de::Error>(self) -> Result<(), E> {
        self.visit_unit()
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        self.0.push(b'[');
        let mut first = true;
        loop {
            let mark = self.0.len();
            if !first {
                self.0.push(b',');
            }
            if seq.next_element_seed(Transcode(&mut *self.0))?.is_none() {
                self.0.truncate(mark);
                break;
            }
            first = false;
        }
        self.0.push(b']');
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        self.0.push(b'{');
        let mut first = true;
        while let Some(key) = map.next_key::<Cow<'de, str>>()? {
            if !first {
                self.0.push(b',');
            }
            first = false;
            sonic_rs::to_writer(&mut *self.0, key.as_ref()).map_err(de::Error::custom)?;
            self.0.push(b':');
            map.next_value_seed(Transcode(&mut *self.0))?;
        }
        self.0.push(b'}');
        Ok(())
    }
}

/// msgpack side: the JSON text arrives as `bin` (Go) or `str`.
struct RawBytesVisitor;

impl Visitor<'_> for RawBytesVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("raw JSON bytes")
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Value, E> {
        sonic_rs::from_slice(v).map_err(E::custom)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        sonic_rs::from_str(v).map_err(E::custom)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::new_null())
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::new_null())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonic_path_preserves_raw_order() {
        let raw = r#"{"z":1,"a":[true,null,"s\u00e9",1.5],"m":{"k":-3,"b":{}}}"#;
        let ext: Ext = sonic_rs::from_str(raw).expect("sonic parse");
        assert_eq!(
            sonic_rs::to_string(&ext).expect("sonic write"),
            r#"{"z":1,"a":[true,null,"sé",1.5],"m":{"k":-3,"b":{}}}"#
        );
    }

    #[test]
    fn other_deserializers_transcode_in_order() {
        let raw = r#"{"z":1,"a":[true,null,"s\"q",1.5,[]],"m":{"k":-3,"e":{}}}"#;
        let ext: Ext = serde_json::from_str(raw).expect("serde_json parse");
        assert_eq!(serde_json::to_string(&ext).expect("serde_json write"), raw);
        let value: Value = sonic_rs::from_str(raw).expect("value");
        let ext: Ext = sonic_rs::from_value(&value).expect("from_value");
        assert_eq!(ext.to_json(), raw);
    }

    #[test]
    fn decode_and_from_serialize() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Prebid {
            channel: String,
        }
        let ext = Ext::from_serialize(&Prebid {
            channel: "pbjs".into(),
        })
        .expect("to value");
        assert_eq!(ext.to_json(), r#"{"channel":"pbjs"}"#);
        assert_eq!(
            ext.decode::<Prebid>().expect("decode"),
            Prebid {
                channel: "pbjs".into()
            }
        );
    }
}
