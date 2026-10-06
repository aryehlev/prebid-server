//! Lenient field (de)serializers shared by every OpenRTB struct.
//!
//! Go's `encoding/json` treats `null` as "leave the zero value" for any field. On top of that,
//! filtration (`filtration-rs/src/types/flexible_types.rs`) already accepts numbers sent as strings
//! and strings sent as numbers. The seller must accept at least what filtration forwards, so the
//! rules here are:
//!
//! - integers and integer codes: a JSON number (a float is truncated), a numeric string
//!   (unparseable means 0), or `null` (0). Values that do not fit the Go width are an error, as in Go.
//! - floats: a number, a numeric string (unparseable means 0), a bool or `null` (0).
//! - strings: a string, a number (formatted), or `null` (empty).
//! - arrays: `null` means empty. Integer arrays also accept a comma-separated string
//!   (filtration's `FlexibleInt8Array`).
//!
//! The visitors go through `deserialize_any`, so they work with `sonic_rs`, `serde_json` and
//! `rmp_serde` alike.

use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use serde::de::{self, DeserializeOwned, Deserializer, SeqAccess, Visitor};
use serde::Deserialize;

thread_local! {
    static STRICT_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// While alive, the lenient visitors below reject what Go's `encoding/json` rejects (a string
/// for a number, a number for a string, a fractional number for an integer). The seller's own
/// request path stays lenient on purpose (filtration forwards such values); parsing a buyer's
/// response through [`crate::jsonutil::unmarshal`] is strict, as Go's `jsonutil.Unmarshal` is.
pub struct StrictScope(());

impl StrictScope {
    pub fn enter() -> Self {
        STRICT_DEPTH.with(|d| d.set(d.get() + 1));
        Self(())
    }
}

impl Drop for StrictScope {
    fn drop(&mut self) {
        STRICT_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

fn strict() -> bool {
    STRICT_DEPTH.with(|d| d.get() > 0)
}

/// `skip_serializing_if` for Go `omitempty` on numbers and codes: true for the zero value.
pub fn is_zero<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// `skip_serializing_if` for a Go slice kept as `Option<Vec<_>>` to tell nil from empty:
/// `omitempty` skips both.
#[allow(clippy::ref_option)]
pub fn is_none_or_empty<T>(value: &Option<Vec<T>>) -> bool {
    value.as_ref().is_none_or(Vec::is_empty)
}

/// Integer field (`int8`, `int`, `int64` in Go). See the module docs for what is accepted.
pub fn int<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: TryFrom<i64>,
{
    let value = deserializer.deserialize_any(IntVisitor)?;
    narrow(value)
}

/// Pointer integer field (`*int8`, `*int64` in Go): `null` means `None`, `0` stays `Some(0)`.
pub fn opt_int<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: TryFrom<i64>,
{
    deserializer.deserialize_option(OptIntVisitor(PhantomData))
}

/// Float field (`float64` in Go).
pub fn float<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(FloatVisitor)
}

/// Pointer float field (`*float64` in Go): `null` means `None`, `0` stays `Some(0.0)`.
pub fn opt_float<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Option<f64>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a number, a numeric string or null")
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            float(d).map(Some)
        }
    }
    deserializer.deserialize_option(V)
}

/// String field.
pub fn string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(StringVisitor)
}

/// `[]string` field. `null` elements become empty strings, as Go does.
pub fn strings<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let items: Vec<LenientString> = seq(deserializer)?;
    Ok(items.into_iter().map(|s| s.0).collect())
}

/// `[]string` field whose Go nil slice must stay distinguishable from `[]` (`null` is `None`).
pub fn opt_strings<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let items: Option<Vec<LenientString>> = Option::deserialize(deserializer)?;
    Ok(items.map(|v| v.into_iter().map(|s| s.0).collect()))
}

/// [`strings`] into a list shared by the request copies (`badv`, OPT-24).
pub fn shared_strings<'de, D>(deserializer: D) -> Result<Arc<[String]>, D::Error>
where
    D: Deserializer<'de>,
{
    strings(deserializer).map(Arc::from)
}

/// Integer array field (`[]int64`, `[]int8`). Also accepts `"2,6"`.
pub fn ints<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: TryFrom<i64>,
{
    deserializer.deserialize_any(IntSeqVisitor(PhantomData))
}

/// Array of anything with its own `Deserialize` (structs, codes): `null` means empty.
pub fn seq<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

/// Non-pointer struct field (e.g. native response `link`): `null` means the zero value.
pub fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

fn narrow<T: TryFrom<i64>, E: de::Error>(value: i64) -> Result<T, E> {
    T::try_from(value).map_err(|_| {
        E::invalid_value(
            de::Unexpected::Signed(value),
            &"an integer in range for the field",
        )
    })
}

/// Go-compatible-ish parse of a numeric string: integer, else float truncated, else 0.
#[allow(clippy::cast_possible_truncation)]
fn parse_int_str(s: &str) -> i64 {
    let s = s.trim();
    s.parse::<i64>()
        .ok()
        .or_else(|| {
            s.parse::<f64>()
                .ok()
                .filter(|f| f.is_finite())
                .map(|f| f.trunc() as i64)
        })
        .unwrap_or(0)
}

#[allow(clippy::cast_possible_truncation)]
fn truncate_float<E: de::Error>(value: f64) -> Result<i64, E> {
    let truncated = value.trunc();
    // i64::MIN as f64 is exact; i64::MAX as f64 rounds up to 2^63, so the upper bound is exclusive.
    #[allow(clippy::cast_precision_loss)]
    let in_range = truncated >= i64::MIN as f64 && truncated < i64::MAX as f64;
    if in_range {
        Ok(truncated as i64)
    } else {
        Err(E::invalid_value(
            de::Unexpected::Float(value),
            &"an integer",
        ))
    }
}

struct IntVisitor;

impl Visitor<'_> for IntVisitor {
    type Value = i64;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an integer, a numeric string or null")
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> {
        Ok(v)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> {
        i64::try_from(v).map_err(|_| E::invalid_value(de::Unexpected::Unsigned(v), &self))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<i64, E> {
        if strict() && v.fract() != 0.0 {
            return Err(E::invalid_type(de::Unexpected::Float(v), &"an integer"));
        }
        truncate_float(v)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<i64, E> {
        if strict() {
            return Err(E::invalid_type(de::Unexpected::Str(v), &"an integer"));
        }
        Ok(parse_int_str(v))
    }

    fn visit_unit<E: de::Error>(self) -> Result<i64, E> {
        Ok(0)
    }

    fn visit_none<E: de::Error>(self) -> Result<i64, E> {
        Ok(0)
    }
}

struct OptIntVisitor<T>(PhantomData<T>);

impl<'de, T: TryFrom<i64>> Visitor<'de> for OptIntVisitor<T> {
    type Value = Option<T>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an integer, a numeric string or null")
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        int(deserializer).map(Some)
    }
}

struct FloatVisitor;

impl Visitor<'_> for FloatVisitor {
    type Value = f64;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a number, a numeric string or null")
    }

    #[allow(clippy::cast_precision_loss)]
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> {
        Ok(v as f64)
    }

    #[allow(clippy::cast_precision_loss)]
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> {
        Ok(v as f64)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> {
        Ok(v)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
        if strict() {
            return Err(E::invalid_type(de::Unexpected::Str(v), &"a number"));
        }
        Ok(v.trim()
            .parse::<f64>()
            .ok()
            .filter(|f| f.is_finite())
            .unwrap_or(0.0))
    }

    fn visit_bool<E: de::Error>(self, b: bool) -> Result<f64, E> {
        if strict() {
            return Err(E::invalid_type(de::Unexpected::Bool(b), &"a number"));
        }
        Ok(0.0)
    }

    fn visit_unit<E: de::Error>(self) -> Result<f64, E> {
        Ok(0.0)
    }

    fn visit_none<E: de::Error>(self) -> Result<f64, E> {
        Ok(0.0)
    }
}

struct StringVisitor;

impl Visitor<'_> for StringVisitor {
    type Value = String;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a string, a number or null")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
        Ok(v.to_owned())
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<String, E> {
        Ok(v)
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<String, E> {
        if strict() {
            return Err(E::invalid_type(de::Unexpected::Signed(v), &"a string"));
        }
        Ok(v.to_string())
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<String, E> {
        if strict() {
            return Err(E::invalid_type(de::Unexpected::Unsigned(v), &"a string"));
        }
        Ok(v.to_string())
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<String, E> {
        if strict() {
            return Err(E::invalid_type(de::Unexpected::Float(v), &"a string"));
        }
        Ok(v.to_string())
    }

    fn visit_unit<E: de::Error>(self) -> Result<String, E> {
        Ok(String::new())
    }

    fn visit_none<E: de::Error>(self) -> Result<String, E> {
        Ok(String::new())
    }
}

struct LenientString(String);

impl<'de> Deserialize<'de> for LenientString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        string(deserializer).map(Self)
    }
}

struct LenientInt(i64);

impl<'de> Deserialize<'de> for LenientInt {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(IntVisitor).map(Self)
    }
}

struct IntSeqVisitor<T>(PhantomData<T>);

impl<'de, T: TryFrom<i64>> Visitor<'de> for IntSeqVisitor<T> {
    type Value = Vec<T>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an array of integers, a comma-separated string or null")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
        let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(64));
        while let Some(LenientInt(value)) = seq.next_element()? {
            out.push(narrow(value)?);
        }
        Ok(out)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Vec<T>, E> {
        v.split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.parse::<i64>().ok())
            .map(narrow)
            .collect()
    }

    fn visit_unit<E: de::Error>(self) -> Result<Vec<T>, E> {
        Ok(Vec::new())
    }

    fn visit_none<E: de::Error>(self) -> Result<Vec<T>, E> {
        Ok(Vec::new())
    }
}

/// Integer code newtype (AdCOM / OpenRTB / Native enumerations).
///
/// Codes are open lists in the specs and buyers send vendor-specific values, so they are
/// newtypes over the Go integer width with named constants rather than closed Rust enums:
/// unknown values survive a round trip. Deserialization uses [`int`].
macro_rules! ortb_code {
    (
        $(#[$meta:meta])*
        $name:ident($repr:ty) {
            $( $(#[$cmeta:meta])* $cname:ident = $val:expr ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub $repr);

        impl $name {
            $( $(#[$cmeta])* pub const $cname: Self = Self($val); )*
        }

        impl From<$repr> for $name {
            fn from(value: $repr) -> Self {
                Self(value)
            }
        }

        impl From<$name> for $repr {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                ::serde::Serialize::serialize(&self.0, serializer)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                $crate::ortb::de::int(deserializer).map(Self)
            }
        }
    };
}

pub(crate) use ortb_code;

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Default, Deserialize)]
    #[serde(default)]
    struct Probe {
        #[serde(deserialize_with = "int")]
        small: i8,
        #[serde(deserialize_with = "int")]
        big: i64,
        #[serde(deserialize_with = "opt_int")]
        ptr: Option<i8>,
        #[serde(deserialize_with = "float")]
        price: f64,
        #[serde(deserialize_with = "string")]
        name: String,
        #[serde(deserialize_with = "strings")]
        names: Vec<String>,
        #[serde(deserialize_with = "ints")]
        sids: Vec<i8>,
    }

    fn probe(json: &str) -> Probe {
        sonic_rs::from_str(json).expect("probe parses with sonic_rs")
    }

    #[test]
    fn numbers_accept_strings_floats_and_null() {
        let p =
            probe(r#"{"small":"7","big":12.9,"ptr":"0","price":"1.25","name":5,"sids":"2, 6"}"#);
        assert_eq!(p.small, 7);
        assert_eq!(p.big, 12);
        assert_eq!(p.ptr, Some(0));
        assert!((p.price - 1.25).abs() < f64::EPSILON);
        assert_eq!(p.name, "5");
        assert_eq!(p.sids, vec![2, 6]);

        let p = probe(
            r#"{"small":null,"big":"abc","ptr":null,"price":true,"name":null,"names":null,"sids":null}"#,
        );
        assert_eq!(p.small, 0);
        assert_eq!(p.big, 0);
        assert_eq!(p.ptr, None);
        assert!(p.price.abs() < f64::EPSILON);
        assert!(p.name.is_empty());
        assert!(p.names.is_empty());
        assert!(p.sids.is_empty());
    }

    #[test]
    fn null_string_elements_become_empty_like_go() {
        let p = probe(r#"{"names":["a",null,3]}"#);
        assert_eq!(p.names, vec!["a".to_owned(), String::new(), "3".to_owned()]);
    }

    #[test]
    fn out_of_range_int8_is_an_error_like_go() {
        assert!(sonic_rs::from_str::<Probe>(r#"{"small":300}"#).is_err());
        assert!(sonic_rs::from_str::<Probe>(r#"{"sids":[1,128]}"#).is_err());
    }

    #[test]
    fn works_with_serde_json_too() {
        let p: Probe =
            serde_json::from_str(r#"{"small":"3","ptr":1,"names":["x"]}"#).expect("parses");
        assert_eq!((p.small, p.ptr, p.names.len()), (3, Some(1), 1));
    }
}
