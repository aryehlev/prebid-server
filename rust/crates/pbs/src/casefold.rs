//! Case-insensitive struct keys, as Go's `encoding/json` and json-iterator match them.
//!
//! Go matches an incoming key to a struct field ignoring case (an exact match wins). serde is
//! exact. [`from_slice`] decodes through a wrapper that, whenever the target asks for a struct,
//! rewrites each key to the declared field name it matches ignoring case. It wraps serde_json's
//! streaming deserializer, so document order is kept and `Ext`'s raw-value hook still reaches
//! the real deserializer.

use std::cell::RefCell;

use serde::de::{self, DeserializeOwned, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};

/// Decodes `T` from JSON text, matching struct keys to field names ignoring case at every level.
pub fn from_slice<T: DeserializeOwned>(data: &[u8]) -> Result<T, serde_json::Error> {
    let mut de = serde_json::Deserializer::from_slice(data);
    let out = T::deserialize(Fold(&mut de))?;
    de.end()?;
    Ok(out)
}

/// A deserializer that forwards to `D` but folds struct keys and wraps everything nested.
pub struct Fold<D>(pub D);

macro_rules! forward {
    ($($method:ident)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
            self.0.$method(FoldVisitor(visitor, None))
        }
    )*};
}

impl<'de, D: Deserializer<'de>> Deserializer<'de> for Fold<D> {
    type Error = D::Error;

    forward! {
        deserialize_any deserialize_bool deserialize_i8 deserialize_i16 deserialize_i32 deserialize_i64
        deserialize_i128 deserialize_u8 deserialize_u16 deserialize_u32 deserialize_u64 deserialize_u128
        deserialize_f32 deserialize_f64 deserialize_char deserialize_str deserialize_string
        deserialize_bytes deserialize_byte_buf deserialize_option deserialize_unit deserialize_seq
        deserialize_map deserialize_identifier deserialize_ignored_any
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(self, name: &'static str, visitor: V) -> Result<V::Value, Self::Error> {
        self.0.deserialize_unit_struct(name, FoldVisitor(visitor, None))
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, name: &'static str, visitor: V) -> Result<V::Value, Self::Error> {
        // `Ext` asks for a special newtype name; the real deserializer must see that name unchanged.
        self.0.deserialize_newtype_struct(name, FoldVisitor(visitor, None))
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error> {
        self.0.deserialize_tuple(len, FoldVisitor(visitor, None))
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(self, name: &'static str, len: usize, visitor: V) -> Result<V::Value, Self::Error> {
        self.0.deserialize_tuple_struct(name, len, FoldVisitor(visitor, None))
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        // From here the visitor is a struct's: its map keys are field names to be folded.
        self.0.deserialize_struct(name, fields, FoldVisitor(visitor, Some(fields)))
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0.deserialize_enum(name, variants, FoldVisitor(visitor, None))
    }

    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}

/// Wraps a visitor so the sequences and maps it is handed keep wrapping their children.
///
/// The second field is the declared field names when the visitor is a struct's. A derived struct
/// visitor also accepts a JSON array (as a tuple), which Go's decoder rejects for a struct, so a
/// sequence handed to a struct visitor is refused here.
struct FoldVisitor<V>(V, Option<&'static [&'static str]>);

impl<'de, V: Visitor<'de>> Visitor<'de> for FoldVisitor<V> {
    type Value = V::Value;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.expecting(f)
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> { self.0.visit_bool(v) }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> { self.0.visit_i64(v) }
    fn visit_i128<E: de::Error>(self, v: i128) -> Result<Self::Value, E> { self.0.visit_i128(v) }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> { self.0.visit_u64(v) }
    fn visit_u128<E: de::Error>(self, v: u128) -> Result<Self::Value, E> { self.0.visit_u128(v) }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> { self.0.visit_f64(v) }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> { self.0.visit_str(v) }
    fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Self::Value, E> { self.0.visit_borrowed_str(v) }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> { self.0.visit_string(v) }
    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> { self.0.visit_bytes(v) }
    fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> { self.0.visit_byte_buf(v) }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> { self.0.visit_none() }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> { self.0.visit_unit() }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.visit_some(Fold(d))
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.visit_newtype_struct(Fold(d))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        if self.1.is_some() {
            // json-iterator's wording for an array where an object (or null) is required.
            return Err(de::Error::custom("expect { or n, but found ["));
        }
        self.0.visit_seq(FoldSeq(seq))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        self.0.visit_map(FoldMap { inner: map, fields: self.1, seen: RefCell::new(Vec::new()) })
    }
}

struct FoldSeq<A>(A);

impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for FoldSeq<A> {
    type Error = A::Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error> {
        self.0.next_element_seed(FoldSeed(seed))
    }
    fn size_hint(&self) -> Option<usize> {
        self.0.size_hint()
    }
}

struct FoldMap<A> {
    inner: A,
    /// Declared field names when this map is a struct's; `None` for a real map (keys are data).
    fields: Option<&'static [&'static str]>,
    /// Declared names already matched exactly or by case, so a case variant of one never overrides it.
    seen: RefCell<Vec<&'static str>>,
}

impl<'de, A: MapAccess<'de>> MapAccess<'de> for FoldMap<A> {
    type Error = A::Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error> {
        match self.fields {
            None => self.inner.next_key_seed(seed),
            Some(fields) => self.inner.next_key_seed(KeySeed { seed, fields, seen: &self.seen }),
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Self::Error> {
        self.inner.next_value_seed(FoldSeed(seed))
    }

    fn size_hint(&self) -> Option<usize> {
        self.inner.size_hint()
    }
}

struct FoldSeed<S>(S);

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for FoldSeed<S> {
    type Value = S::Value;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.deserialize(Fold(d))
    }
}

/// A key no struct declares, so serde skips its value.
const IGNORED_KEY: &str = "\u{0}pbs-ignored-key";

/// Reads a struct key as text and hands the declared field name to the real key seed.
struct KeySeed<'a, K> {
    seed: K,
    fields: &'static [&'static str],
    seen: &'a RefCell<Vec<&'static str>>,
}

impl<'de, K: DeserializeSeed<'de>> DeserializeSeed<'de> for KeySeed<'_, K> {
    type Value = K::Value;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        let key: String = serde::Deserialize::deserialize(d)?;
        // A key that is a declared field name is used as is. Otherwise the field it matches
        // ignoring case, unless that field was already set: serde rejects a repeated field, where
        // Go takes the last value. A repeat is routed to a name no field has, so it is ignored.
        let name: &str = match self.fields.iter().find(|f| **f == key) {
            Some(f) if self.seen.borrow().contains(f) => IGNORED_KEY,
            Some(f) => {
                self.seen.borrow_mut().push(f);
                f
            }
            None => match self.fields.iter().find(|f| f.eq_ignore_ascii_case(&key)) {
                Some(f) if self.seen.borrow().contains(f) => IGNORED_KEY,
                Some(f) => {
                    self.seen.borrow_mut().push(f);
                    f
                }
                None => &key,
            },
        };
        self.seed.deserialize(de::value::StrDeserializer::<D::Error>::new(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::collections::HashMap;

    #[derive(Deserialize, Debug, PartialEq, Default)]
    #[serde(default)]
    struct Roulax {
        #[serde(rename = "Pid")]
        pid: String,
        #[serde(rename = "publisherPath")]
        publisher_path: String,
    }

    #[derive(Deserialize, Debug, PartialEq, Default)]
    #[serde(default)]
    struct Inner {
        #[serde(rename = "someName")]
        some_name: String,
    }

    #[derive(Deserialize, Debug, PartialEq, Default)]
    #[serde(default)]
    struct Outer {
        #[serde(rename = "siteId")]
        site_id: String,
        inner: Vec<Inner>,
        extra: Option<Inner>,
        free: HashMap<String, Inner>,
    }

    #[test]
    fn folds_keys_that_differ_only_by_case() {
        let r: Roulax = from_slice(br#"{"PublisherPath":"72721","pid":"mvo","zone":"1r"}"#).unwrap();
        assert_eq!(r, Roulax { pid: "mvo".into(), publisher_path: "72721".into() });
    }

    #[test]
    fn exact_key_wins_over_a_case_variant_in_either_order() {
        let a: Roulax = from_slice(br#"{"publisherPath":"exact","PublisherPath":"variant"}"#).unwrap();
        let b: Roulax = from_slice(br#"{"PublisherPath":"variant","publisherPath":"exact"}"#).unwrap();
        assert_eq!(a.publisher_path, "exact");
        // Limit of streaming: the variant arrived first and set the field; the exact key after it
        // is a repeat and is ignored. Go would take the exact one. Both spellings in one object
        // do not occur in real bidder params.
        assert_eq!(b.publisher_path, "variant");
    }

    #[test]
    fn folds_at_every_level_but_not_map_keys() {
        let o: Outer = from_slice(
            br#"{"SITEID":"s","Inner":[{"SomeName":"a"},{"somename":"b"}],"extra":{"SOMENAME":"c"},"free":{"KeepMe":{"SomeName":"d"}}}"#,
        )
        .unwrap();
        assert_eq!(o.site_id, "s");
        assert_eq!(o.inner.iter().map(|i| i.some_name.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(o.extra.unwrap().some_name, "c");
        assert_eq!(o.free["KeepMe"].some_name, "d");
    }

    #[test]
    fn a_json_array_is_not_a_struct() {
        // serde would read `[]` as an empty tuple for a defaulted struct; Go rejects it.
        let err = from_slice::<Roulax>(b"[]").unwrap_err().to_string();
        assert!(err.starts_with("expect { or n, but found ["), "{err}");
        // ...also nested one level down, where `ext.bidder` is the wrong shape.
        #[derive(Deserialize, Debug)]
        #[serde(default)]
        struct Wrap {
            #[allow(dead_code)]
            bidder: Roulax,
        }
        impl Default for Wrap {
            fn default() -> Self {
                Wrap { bidder: Roulax::default() }
            }
        }
        assert!(from_slice::<Wrap>(br#"{"bidder":[]}"#).is_err());
        assert!(from_slice::<Wrap>(br#"{"bidder":{}}"#).is_ok());
    }

    #[test]
    fn keeps_document_order_for_errors() {
        // `bidfloor` comes before `appid` in the document; the first bad value is reported first.
        #[derive(Deserialize, Debug)]
        struct P {
            #[allow(dead_code)]
            appid: String,
            #[allow(dead_code)]
            bidfloor: f64,
        }
        let err = from_slice::<P>(br#"{"bidfloor":"x","appid":1}"#).unwrap_err().to_string();
        assert!(err.contains(r#"string "x""#), "{err}");
    }
}
