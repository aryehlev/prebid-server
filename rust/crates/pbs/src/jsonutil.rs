//! Go `util/jsonutil.Unmarshal`: parse into a typed struct, mapping failures to
//! `FailedToUnmarshal` with json-iterator's wording where the fixtures depend on it.
//!
//! Only the top-level shape errors are reproduced exactly (`expect { or n, but found X`, which
//! ~130 fixtures compare). Errors deeper in a document come out in serde's wording, so a fixture
//! expecting a Go struct path such as `cannot unmarshal openrtb2.Bid.W: ...` will not match.

use serde::de::DeserializeOwned;

use crate::errortypes::BidderError;

/// Go `jsonutil.Unmarshal(data, &v)` into a struct or map.
///
/// json-iterator picks the object decoder from the first non-space byte, so anything but `{` or
/// `null` fails there. serde would accept a JSON array for a derived struct (as a tuple), so the
/// byte is checked up front instead of leaving it to the deserializer.
pub fn unmarshal<T: DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    // Empty or all-space input reads as byte 0 (json-iterator's end-of-input).
    let first = data.iter().find(|b| !b" \t\r\n".contains(b)).copied().unwrap_or(0);
    if first != b'{' && first != b'n' {
        return Err(BidderError::FailedToUnmarshal(format!(
            "expect {{ or n, but found {}",
            first as char
        )));
    }
    // A literal `null` is valid for an object target in Go and leaves the zero value (the later
    // checks, e.g. an empty `bidder`, are where it fails). serde rejects `null` for a struct, so
    // read it as `{}`: that is the zero value for a struct whose fields all default. A struct with a
    // required field cannot be built that way and keeps serde's error.
    if first == b'n' && data.iter().all(|b| b" \t\r\n".contains(b) || b"null".contains(b)) {
        if let Ok(v) = unmarshal_any::<T>(b"{}") {
            return Ok(v);
        }
    }
    unmarshal_any(data)
}

/// Go `jsonutil.Unmarshal` into a non-object target (slice, scalar), or after the object check.
///
/// Buyer data is parsed as strictly as Go does it (see `StrictScope`). A failure inside a bid
/// response is reworded like json-iterator (`cannot unmarshal openrtb2.Bid.W: ...`); anything
/// else keeps serde's wording.
pub fn unmarshal_any<T: DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    let _strict = crate::ortb::de::StrictScope::enter();
    let mut de = serde_json::Deserializer::from_slice(data);
    serde_path_to_error::deserialize(&mut de).map_err(|e| {
        let msg = go_message(e.path().to_string().as_str(), e.inner()).unwrap_or_else(|| e.inner().to_string());
        BidderError::FailedToUnmarshal(msg)
    })
}

/// Kind of a Go field, which decides json-iterator's reason text.
#[derive(Clone, Copy)]
enum Kind {
    Int,
    Float,
    Str,
    Slice,
}

/// `(json key, Go field name, kind)` for the structs of a buyer's `openrtb2.BidResponse`.
const BID: &[(&str, &str, Kind)] = &[
    ("id", "ID", Kind::Str), ("impid", "ImpID", Kind::Str), ("price", "Price", Kind::Float),
    ("nurl", "NURL", Kind::Str), ("burl", "BURL", Kind::Str), ("lurl", "LURL", Kind::Str),
    ("adm", "AdM", Kind::Str), ("adid", "AdID", Kind::Str), ("adomain", "ADomain", Kind::Slice),
    ("bundle", "Bundle", Kind::Str), ("iurl", "IURL", Kind::Str), ("cid", "CID", Kind::Str),
    ("crid", "CrID", Kind::Str), ("tactic", "Tactic", Kind::Str), ("cattax", "CatTax", Kind::Int),
    ("cat", "Cat", Kind::Slice), ("attr", "Attr", Kind::Slice), ("apis", "APIs", Kind::Slice),
    ("api", "API", Kind::Int), ("protocol", "Protocol", Kind::Int),
    ("qagmediarating", "QAGMediaRating", Kind::Int), ("language", "Language", Kind::Str),
    ("langb", "LangB", Kind::Str), ("dealid", "DealID", Kind::Str), ("w", "W", Kind::Int),
    ("h", "H", Kind::Int), ("wratio", "WRatio", Kind::Int), ("hratio", "HRatio", Kind::Int),
    ("exp", "Exp", Kind::Int), ("dur", "Dur", Kind::Int), ("mtype", "MType", Kind::Int),
    ("slotinpod", "SlotInPod", Kind::Int),
];
const SEAT_BID: &[(&str, &str, Kind)] = &[
    ("bid", "Bid", Kind::Slice), ("seat", "Seat", Kind::Str), ("group", "Group", Kind::Int),
];
const BID_RESPONSE: &[(&str, &str, Kind)] = &[
    ("id", "ID", Kind::Str), ("seatbid", "SeatBid", Kind::Slice), ("bidid", "BidID", Kind::Str),
    ("cur", "Cur", Kind::Str), ("customdata", "CustomData", Kind::Str),
];

/// json-iterator's wording for a type mismatch inside a bid response, from serde's JSON path
/// (`seatbid[0].bid[0].w`) and error. `None` for anything not in the tables above.
fn go_message(path: &str, err: &serde_json::Error) -> Option<String> {
    let segments: Vec<&str> = path.split('.').collect();
    let leaf = segments.last()?;
    let leaf = leaf.split('[').next()?;
    // The struct is decided by how deep the path is: `bid[i].x` is a Bid, `seatbid[i].x` is a
    // SeatBid, a top-level key is the BidResponse.
    let (strukt, table) = match segments.len() {
        1 => ("BidResponse", BID_RESPONSE),
        2 if segments[0].starts_with("seatbid") => ("SeatBid", SEAT_BID),
        3 if segments[1].starts_with("bid") => ("Bid", BID),
        _ => return None,
    };
    let (_, go_name, kind) = table.iter().find(|(k, _, _)| *k == leaf)?;
    let found = found_char(&err.to_string())?;
    let reason = match kind {
        Kind::Int => "unexpected character".to_string(),
        Kind::Float => "invalid number".to_string(),
        Kind::Str => format!("expects \" or n, but found {found}"),
        Kind::Slice => format!("decode slice: expect [ or n, but found {found}"),
    };
    Some(format!("cannot unmarshal openrtb2.{strukt}.{go_name}: {reason}"))
}

/// First character of the offending JSON value, from serde's `invalid type: ...` text.
fn found_char(msg: &str) -> Option<char> {
    let rest = msg.strip_prefix("invalid type: ")?;
    let first = rest.split(',').next()?;
    if let Some(n) = first.strip_prefix("integer `").or_else(|| first.strip_prefix("floating point `")) {
        n.chars().next()
    } else if first.starts_with("string") {
        Some('"')
    } else if first.starts_with("map") {
        Some('{')
    } else if first.starts_with("sequence") {
        Some('[')
    } else if first.starts_with("boolean") {
        Some(if first.contains("true") { 't' } else { 'f' })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ortb::openrtb2::BidResponse;

    #[test]
    fn quoted_string_body_reports_the_quote() {
        // adapters/adf/adftest/supplemental/unparsable-response.json: body `""`.
        let err = unmarshal::<BidResponse>(b"\"\"").unwrap_err();
        assert_eq!(err.to_string(), "expect { or n, but found \"");
        assert_eq!(err.code(), crate::errortypes::FAILED_TO_UNMARSHAL_ERROR_CODE);
    }

    #[test]
    fn array_body_reports_the_bracket() {
        let err = unmarshal::<BidResponse>(b"[]").unwrap_err();
        assert_eq!(err.to_string(), "expect { or n, but found [");
    }
}
