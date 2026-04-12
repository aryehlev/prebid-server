use serde_json::Value;

use crate::types::ImpsWithBidResponses;

/// Drop any imps from `bid_request.imp` that have a matching entry in
/// `imps_with` so they will not be sent to real adapters. Stored auction
/// responses will be injected after adapter auction completes.
pub fn remove_imps_with_stored_responses(
    bid_request: &mut Value,
    imps_with: &ImpsWithBidResponses,
) {
    if imps_with.is_empty() {
        return;
    }
    let Some(imps) = bid_request.get_mut("imp").and_then(|v| v.as_array_mut()) else {
        return;
    };
    imps.retain(|imp| {
        imp.get("id")
            .and_then(|v| v.as_str())
            .map(|id| !imps_with.contains_key(id))
            .unwrap_or(true)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn filters_matching_imps() {
        let mut req = json!({
            "imp": [
                { "id": "imp-1" },
                { "id": "imp-2" },
                { "id": "imp-3" }
            ]
        });
        let mut imps_with = HashMap::new();
        imps_with.insert("imp-1".to_string(), json!({}));
        imps_with.insert("imp-3".to_string(), json!({}));

        remove_imps_with_stored_responses(&mut req, &imps_with);

        let imps = req.get("imp").unwrap().as_array().unwrap();
        assert_eq!(imps.len(), 1);
        assert_eq!(imps[0].get("id").unwrap().as_str().unwrap(), "imp-2");
    }

    #[test]
    fn noop_when_empty() {
        let mut req = json!({ "imp": [ { "id": "imp-1" } ] });
        let imps_with = HashMap::new();
        remove_imps_with_stored_responses(&mut req, &imps_with);
        assert_eq!(req["imp"].as_array().unwrap().len(), 1);
    }
}
