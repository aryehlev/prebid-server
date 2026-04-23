//! Structured generators for Prebid-shaped requests.
//!
//! These helpers are intended to be driven either by `arbitrary` (for
//! unstructured fuzzing) or called directly from unit tests.

use arbitrary::Arbitrary;
use serde_json::{json, Value};

/// A compact, fuzz-friendly description of a Prebid bid request.
///
/// This struct derives [`Arbitrary`] so it can be synthesized from a raw
/// byte buffer by libFuzzer/AFL-style harnesses.
#[derive(Debug, Clone, Arbitrary)]
pub struct ArbRequest {
    /// Request id. Truncated to a safe ASCII slice before use.
    pub id: String,
    /// Number of impressions to generate. Clamped to `1..=8`.
    pub imp_count: u8,
    /// Timeout in milliseconds. Clamped to `1..=5000`.
    pub tmax: u16,
    /// Whether to include a `site` object.
    pub has_site: bool,
    /// Whether to include an `app` object (OpenRTB forbids both — we
    /// prefer `site` if both are set).
    pub has_app: bool,
    /// List of currency codes. Empty is normalized to `["USD"]`.
    pub currencies: Vec<String>,
}

/// Convert an [`ArbRequest`] into a minimally valid OpenRTB-shaped
/// `serde_json::Value`.
pub fn generate_request(arb: &ArbRequest) -> Value {
    let id = sanitize_id(&arb.id);
    let imp_count = ((arb.imp_count as usize) % 8).max(1);
    let tmax = ((arb.tmax as u32) % 5000).max(1);

    let imps: Vec<Value> = (0..imp_count)
        .map(|i| generate_imp(&format!("imp-{i}"), i as u64))
        .collect();

    let mut req = json!({
        "id": id,
        "imp": imps,
        "tmax": tmax,
        "at": 1,
    });

    // Prefer site over app if both flags are set.
    if arb.has_site || (!arb.has_site && !arb.has_app) {
        req.as_object_mut().unwrap().insert(
            "site".to_string(),
            json!({
                "id": "site-1",
                "domain": "example.com",
                "page": "https://example.com/",
            }),
        );
    } else if arb.has_app {
        req.as_object_mut().unwrap().insert(
            "app".to_string(),
            json!({
                "id": "app-1",
                "bundle": "com.example.app",
                "name": "Example",
            }),
        );
    }

    let cur: Vec<Value> = if arb.currencies.is_empty() {
        vec![json!("USD")]
    } else {
        arb.currencies
            .iter()
            .map(|c| {
                let clean: String = c
                    .chars()
                    .filter(|ch| ch.is_ascii_alphabetic())
                    .take(3)
                    .collect::<String>()
                    .to_uppercase();
                if clean.len() == 3 {
                    json!(clean)
                } else {
                    json!("USD")
                }
            })
            .collect()
    };
    req.as_object_mut().unwrap().insert("cur".into(), Value::Array(cur));

    req
}

/// Generate a single `imp` object with a deterministic format based on
/// `seed` — 0: banner, 1: video, 2: native, then cycles.
pub fn generate_imp(id: &str, seed: u64) -> Value {
    let variant = seed % 3;
    let mut imp = json!({
        "id": id,
        "secure": 1,
        "ext": {
            "prebid": {
                "bidder": {
                    "appnexus": { "placement_id": 12345 }
                }
            }
        }
    });

    match variant {
        0 => {
            imp.as_object_mut().unwrap().insert(
                "banner".to_string(),
                json!({
                    "w": 300,
                    "h": 250,
                    "format": [{ "w": 300, "h": 250 }, { "w": 728, "h": 90 }]
                }),
            );
        }
        1 => {
            imp.as_object_mut().unwrap().insert(
                "video".to_string(),
                json!({
                    "mimes": ["video/mp4"],
                    "w": 640,
                    "h": 480,
                    "minduration": 5,
                    "maxduration": 30,
                    "protocols": [2, 3, 5, 6]
                }),
            );
        }
        _ => {
            imp.as_object_mut().unwrap().insert(
                "native".to_string(),
                json!({
                    "request": "{\"ver\":\"1.2\",\"assets\":[]}",
                    "ver": "1.2"
                }),
            );
        }
    }

    imp
}

/// Strip a generated id down to printable ASCII, bounded length.
fn sanitize_id(raw: &str) -> String {
    let clean: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(32)
        .collect();
    if clean.is_empty() {
        "req-0".to_string()
    } else {
        clean
    }
}

/// Canonical 20-bidder pool used when seeding the `ext.prebid.bidder` map
/// for broad coverage tests.
pub fn bidder_pool() -> &'static [&'static str] {
    const POOL: &[&str] = &[
        "appnexus",
        "rubicon",
        "pubmatic",
        "openx",
        "sovrn",
        "ix",
        "criteo",
        "adform",
        "smartadserver",
        "gumgum",
        "yieldmo",
        "triplelift",
        "33across",
        "sharethrough",
        "beachfront",
        "unruly",
        "conversant",
        "adkernel",
        "consumable",
        "telaria",
    ];
    POOL
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbitrary::{Arbitrary, Unstructured};

    #[test]
    fn arb_request_implements_arbitrary() {
        // Provide a deterministic byte buffer and ensure construction succeeds.
        let raw = [0xABu8; 128];
        let mut u = Unstructured::new(&raw);
        let req = ArbRequest::arbitrary(&mut u).expect("arbitrary should succeed");
        // Field presence is enough — we only assert the type constructs.
        let _ = req.id;
        let _ = req.imp_count;
    }

    #[test]
    fn generate_request_produces_valid_json_with_requested_imp_count() {
        let arb = ArbRequest {
            id: "test-req".into(),
            imp_count: 3,
            tmax: 500,
            has_site: true,
            has_app: false,
            currencies: vec!["USD".into(), "EUR".into()],
        };
        let v = generate_request(&arb);
        assert!(v.get("id").is_some());
        assert_eq!(
            v.get("imp").and_then(|i| i.as_array()).map(|a| a.len()),
            Some(3)
        );
        // Round-trip through serde to prove it's valid JSON.
        let s = serde_json::to_string(&v).unwrap();
        let _: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert!(v.get("site").is_some());
        assert!(v.get("cur").and_then(|c| c.as_array()).is_some());
    }

    #[test]
    fn bidder_pool_has_20_entries() {
        assert_eq!(bidder_pool().len(), 20);
    }

    #[test]
    fn generate_imp_produces_banner_video_native_variants() {
        assert!(generate_imp("x", 0).get("banner").is_some());
        assert!(generate_imp("x", 1).get("video").is_some());
        assert!(generate_imp("x", 2).get("native").is_some());
    }
}
