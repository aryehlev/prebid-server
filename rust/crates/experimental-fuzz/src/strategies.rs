//! `proptest::Strategy` builders for valid Prebid JSON objects.

use proptest::collection::vec;
use proptest::prelude::*;
use serde_json::{json, Value};

/// Strategy building a valid `banner` object.
pub fn banner_strategy() -> impl Strategy<Value = Value> {
    (
        prop_oneof![Just(300u32), Just(728u32), Just(160u32), Just(320u32)],
        prop_oneof![Just(250u32), Just(90u32), Just(600u32), Just(50u32)],
    )
        .prop_map(|(w, h)| {
            json!({
                "w": w,
                "h": h,
                "format": [{ "w": w, "h": h }],
            })
        })
}

/// Strategy for a single `imp` object wrapping a banner.
pub fn imp_strategy() -> impl Strategy<Value = Value> {
    ("[a-zA-Z0-9]{1,8}", banner_strategy()).prop_map(|(id, banner)| {
        json!({
            "id": id,
            "secure": 1,
            "banner": banner,
            "ext": {
                "prebid": {
                    "bidder": {
                        "appnexus": { "placement_id": 1 }
                    }
                }
            }
        })
    })
}

/// Strategy for a valid `site` object.
pub fn site_strategy() -> impl Strategy<Value = Value> {
    (
        "[a-z]{3,8}",
        "[a-z]{3,8}\\.(com|net|org)",
    )
        .prop_map(|(id, domain)| {
            json!({
                "id": id,
                "domain": domain,
                "page": format!("https://{domain}/"),
            })
        })
}

/// Strategy for a `user` object.
pub fn user_strategy() -> impl Strategy<Value = Value> {
    ("[a-zA-Z0-9\\-]{4,16}", 13u32..99u32).prop_map(|(id, yob)| {
        json!({
            "id": id,
            "yob": 1900 + yob,
        })
    })
}

/// Compose the sub-strategies into a complete, valid bid request.
pub fn bid_request_strategy() -> impl Strategy<Value = Value> {
    (
        "[a-zA-Z0-9\\-]{4,16}",
        vec(imp_strategy(), 1..4),
        site_strategy(),
        user_strategy(),
        1u32..5000u32,
    )
        .prop_map(|(id, imps, site, user, tmax)| {
            json!({
                "id": id,
                "imp": imps,
                "site": site,
                "user": user,
                "tmax": tmax,
                "at": 1,
                "cur": ["USD"],
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::test_runner::Config;

    proptest! {
        #![proptest_config(Config { cases: 1000, max_shrink_iters: 32, .. Config::default() })]

        #[test]
        fn bid_request_strategy_produces_valid_requests(req in bid_request_strategy()) {
            // Must be an object.
            prop_assert!(req.is_object());
            // Top-level required fields.
            prop_assert!(req.get("id").and_then(|v| v.as_str()).is_some());
            let imps = req.get("imp").and_then(|v| v.as_array()).unwrap();
            prop_assert!(!imps.is_empty());
            for imp in imps {
                prop_assert!(imp.get("id").and_then(|v| v.as_str()).is_some());
                prop_assert!(imp.get("banner").is_some());
            }
            prop_assert!(req.get("site").is_some());
            prop_assert!(req.get("user").is_some());
            // Round-trip through serde_json to prove valid JSON.
            let s = serde_json::to_string(&req).unwrap();
            let back: Value = serde_json::from_str(&s).unwrap();
            prop_assert_eq!(req, back);
        }
    }
}
