//! JSON text as Go's `lib/json` writes it (`jsoniter.ConfigCompatibleWithStandardLibrary`,
//! `lib/json/json.go:11-23`).
//!
//! The seller's `json.Marshal` escapes HTML in strings: `<`, `>` and `&` become `\u003c`,
//! `\u003e` and `\u0026`, and U+2028 / U+2029 become `\u2028` / `\u2029`. Control characters
//! other than `\n`, `\r` and `\t` are `\u00XX` (no `\b` / `\f` short forms;
//! jsoniter `writeStringSlowPathWithHTMLEscaped`). A `json.RawMessage` field (Rust
//! [`Ext`](crate::ortb::Ext)) is written as is, so strings inside it keep the plain
//! escaping. Go wrote publisher responses (`Writer.WriteJSON`), `/exposeRequests`, `/verbose`
//! and buyer request bodies (`GenericBuyer.CreateHTTP2Request` / `SetFastHTTPRequest`) this
//! way, and their measured sizes include the escapes.
//!
//! Floats keep serde's text (`3.0` where Go wrote `3`, the same JSON number).

use std::cell::Cell;
use std::io;

use serde::Serialize;
use serde_json::ser::{CharEscape, Formatter};

thread_local! {
    /// Depth of `json.RawMessage` values being serialized on this thread.
    static RAW_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Marks the serialization of a `json.RawMessage` stand-in: while it lives, [`to_vec`] writes
/// strings without the HTML escapes, as Go's raw passthrough did.
pub struct RawMessageScope(());

impl RawMessageScope {
    pub fn enter() -> Self {
        RAW_DEPTH.with(|depth| depth.set(depth.get() + 1));
        Self(())
    }
}

impl Drop for RawMessageScope {
    fn drop(&mut self) {
        RAW_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

fn in_raw_message() -> bool {
    RAW_DEPTH.with(|depth| depth.get() > 0)
}

/// Go `json.Marshal(value)` (see the module doc).
pub fn to_vec<T: Serialize + ?Sized>(value: &T) -> serde_json::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(256);
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, GoFormatter);
    value.serialize(&mut serializer)?;
    Ok(out)
}

/// serde_json's compact formatter with jsoniter's HTML-safe string escaping.
struct GoFormatter;

impl Formatter for GoFormatter {
    fn write_string_fragment<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        if in_raw_message() {
            return writer.write_all(fragment.as_bytes());
        }
        let mut start = 0;
        for (index, c) in fragment.char_indices() {
            let escape = match c {
                '<' => "\\u003c",
                '>' => "\\u003e",
                '&' => "\\u0026",
                '\u{2028}' => "\\u2028",
                '\u{2029}' => "\\u2029",
                _ => continue,
            };
            writer.write_all(&fragment.as_bytes()[start..index])?;
            writer.write_all(escape.as_bytes())?;
            start = index + c.len_utf8();
        }
        writer.write_all(&fragment.as_bytes()[start..])
    }

    fn write_char_escape<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        char_escape: CharEscape,
    ) -> io::Result<()> {
        if in_raw_message() {
            return serde_json::ser::CompactFormatter.write_char_escape(writer, char_escape);
        }
        match char_escape {
            CharEscape::Backspace => writer.write_all(b"\\u0008"),
            CharEscape::FormFeed => writer.write_all(b"\\u000c"),
            other => serde_json::ser::CompactFormatter.write_char_escape(writer, other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ortb::Ext;

    // jsoniter `writeStringSlowPathWithHTMLEscaped` (ConfigCompatibleWithStandardLibrary,
    // lib/json/json.go:11): `<`, `>`, `&` → `\u00XX`, U+2028/9 → `\u202X`, `\b`/`\f` as
    // `\u0008`/`\u000c`, quotes, backslash, `\n`, `\r`, `\t` as short escapes.
    #[test]
    fn strings_are_html_escaped_like_go() {
        let json = to_vec(&serde_json::json!({
            "adm": "<div a=\"1\">&amp;</div>\u{2028}\u{2029}\u{8}\u{c}\n\r\t\\\u{1}é"
        }))
        .expect("json");
        assert_eq!(
            String::from_utf8(json).expect("utf8"),
            r#"{"adm":"\u003cdiv a=\"1\"\u003e\u0026amp;\u003c/div\u003e\u2028\u2029\u0008\u000c\n\r\t\\\u0001é"}"#
        );
    }

    // jsoniter's `json.RawMessage` codec writes the raw bytes (`stream.WriteRaw`): no HTML
    // escaping inside `ext`.
    #[test]
    fn raw_messages_are_not_escaped() {
        #[derive(Serialize)]
        struct Imp {
            tagid: String,
            ext: Ext,
        }
        let imp = Imp {
            tagid: "a<b".into(),
            ext: Ext::from_slice(br#"{"html":"<b>&</b>","n":[1,-2,0.5,true,null]}"#).expect("ext"),
        };
        assert_eq!(
            String::from_utf8(to_vec(&imp).expect("json")).expect("utf8"),
            r#"{"tagid":"a\u003cb","ext":{"html":"<b>&</b>","n":[1,-2,0.5,true,null]}}"#
        );
    }
}
