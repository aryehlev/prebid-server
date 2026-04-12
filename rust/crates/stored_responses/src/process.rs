use std::collections::HashMap;

use serde_json::Value;

use crate::error::StoredRespError;
use crate::fetcher::StoredResponsesFetcher;
use crate::types::{ImpBidderReplaceImpId, ImpBidderStoredResp, ImpsWithBidResponses};

/// Intermediate structure produced by [`extract_stored_response_ids`].
#[derive(Debug, Default)]
pub(crate) struct ExtractedIds {
    pub all_ids: Vec<String>,
    /// imp id -> stored auction response id
    pub imp_to_auction_resp_id: HashMap<String, String>,
    /// imp id -> bidder -> stored bid response id
    pub imp_bidder_to_resp_id: HashMap<String, HashMap<String, String>>,
    /// imp id -> bidder -> replace imp id flag
    pub imp_bidder_replace_imp: ImpBidderReplaceImpId,
}

/// Walks the bid request and extracts all stored auction response / stored bid
/// response ids declared on each imp's `ext.prebid` block.
pub(crate) fn extract_stored_response_ids(
    bid_request: &Value,
) -> Result<ExtractedIds, StoredRespError> {
    let mut out = ExtractedIds::default();

    let Some(imps) = bid_request.get("imp").and_then(|v| v.as_array()) else {
        return Ok(out);
    };

    for (index, imp) in imps.iter().enumerate() {
        let imp_id = imp
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| StoredRespError::InvalidImp {
                index,
                message: "imp.id is required".to_string(),
            })?
            .to_string();

        let Some(prebid) = imp
            .get("ext")
            .and_then(|e| e.get("prebid"))
        else {
            continue;
        };

        // Stored auction response.
        if let Some(sar) = prebid.get("storedauctionresponse") {
            let id = sar
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if id.is_empty() {
                return Err(StoredRespError::InvalidImp {
                    index,
                    message: "has ext.prebid.storedauctionresponse specified, but \"id\" field is missing".to_string(),
                });
            }
            out.all_ids.push(id.to_string());
            out.imp_to_auction_resp_id
                .insert(imp_id.clone(), id.to_string());
        }

        // Stored bid responses (per-bidder).
        if let Some(sbr_arr) = prebid.get("storedbidresponse").and_then(|v| v.as_array()) {
            // Collect bidder names declared on this imp (imp.ext.prebid.bidder
            // plus top-level bidder keys on imp.ext that aren't "prebid").
            let mut all_bidder_names: Vec<String> = Vec::new();
            if let Some(bidder_obj) = prebid.get("bidder").and_then(|v| v.as_object()) {
                for key in bidder_obj.keys() {
                    all_bidder_names.push(key.clone());
                }
            }
            if let Some(ext_obj) = imp.get("ext").and_then(|v| v.as_object()) {
                for key in ext_obj.keys() {
                    if key != "prebid" {
                        all_bidder_names.push(key.clone());
                    }
                }
            }

            let mut bidder_resp_id: HashMap<String, String> = HashMap::new();
            let mut bidder_replace_imp: HashMap<String, bool> = HashMap::new();

            for bidder_resp in sbr_arr {
                let id = bidder_resp
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let target_bidder = bidder_resp
                    .get("bidder")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if id.is_empty() || target_bidder.is_empty() {
                    return Err(StoredRespError::InvalidImp {
                        index,
                        message: "has ext.prebid.storedbidresponse specified, but \"id\" or/and \"bidder\" fields are missing".to_string(),
                    });
                }

                // replaceimpid defaults to true.
                let replace_imp_id = bidder_resp
                    .get("replaceimpid")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                for bidder_name in &all_bidder_names {
                    if bidder_resp_id.contains_key(bidder_name) {
                        continue;
                    }
                    if bidder_name.eq_ignore_ascii_case(target_bidder) {
                        bidder_resp_id.insert(bidder_name.clone(), id.to_string());
                        bidder_replace_imp.insert(bidder_name.clone(), replace_imp_id);
                        out.all_ids.push(id.to_string());
                    }
                }
            }

            if !bidder_resp_id.is_empty() {
                out.imp_bidder_to_resp_id
                    .insert(imp_id.clone(), bidder_resp_id);
                out.imp_bidder_replace_imp
                    .insert(imp_id.clone(), bidder_replace_imp);
            }
        }
    }

    Ok(out)
}

/// Process a merged bid request: extract stored response ids, fetch them via
/// the supplied fetcher, and return the three resolved maps.
pub async fn process_stored_responses(
    bid_request: &Value,
    fetcher: &dyn StoredResponsesFetcher,
) -> Result<
    (
        ImpsWithBidResponses,
        ImpBidderStoredResp,
        ImpBidderReplaceImpId,
    ),
    StoredRespError,
> {
    let extracted = extract_stored_response_ids(bid_request)?;

    if extracted.all_ids.is_empty() {
        return Ok((
            ImpsWithBidResponses::new(),
            ImpBidderStoredResp::new(),
            ImpBidderReplaceImpId::new(),
        ));
    }

    let fetched = fetcher.fetch_responses(&extracted.all_ids).await?;

    let mut imp_to_stored: ImpsWithBidResponses = HashMap::new();
    for (imp_id, resp_id) in &extracted.imp_to_auction_resp_id {
        match fetched.get(resp_id) {
            Some(body) if !body.is_null() => {
                imp_to_stored.insert(imp_id.clone(), body.clone());
            }
            _ => {
                return Err(StoredRespError::NotFound(format!(
                    "failed to fetch stored auction response for impId = {imp_id} and storedAuctionResponse id = {resp_id}"
                )));
            }
        }
    }

    let mut imp_bidder_stored: ImpBidderStoredResp = HashMap::new();
    for (imp_id, bidder_map) in &extracted.imp_bidder_to_resp_id {
        let mut bidder_bodies: HashMap<String, Value> = HashMap::new();
        for (bidder, id) in bidder_map {
            match fetched.get(id) {
                Some(body) if !body.is_null() => {
                    bidder_bodies.insert(bidder.clone(), body.clone());
                }
                _ => {
                    return Err(StoredRespError::NotFound(format!(
                        "failed to fetch stored bid response for impId = {imp_id}, bidder = {bidder} and storedBidResponse id = {id}"
                    )));
                }
            }
        }
        imp_bidder_stored.insert(imp_id.clone(), bidder_bodies);
    }

    Ok((
        imp_to_stored,
        imp_bidder_stored,
        extracted.imp_bidder_replace_imp,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetcher::InMemoryStoredResponsesFetcher;
    use serde_json::json;

    #[test]
    fn extract_auction_and_bid_responses() {
        let req = json!({
            "imp": [
                {
                    "id": "imp-1",
                    "ext": {
                        "prebid": {
                            "storedauctionresponse": { "id": "sar-1" },
                            "bidder": { "appnexus": {}, "rubicon": {} },
                            "storedbidresponse": [
                                { "id": "sbr-appnexus", "bidder": "appnexus" },
                                { "id": "sbr-rubicon", "bidder": "rubicon", "replaceimpid": false }
                            ]
                        }
                    }
                },
                {
                    "id": "imp-2",
                    "ext": { "prebid": {} }
                }
            ]
        });

        let extracted = extract_stored_response_ids(&req).unwrap();
        assert!(extracted.all_ids.contains(&"sar-1".to_string()));
        assert!(extracted.all_ids.contains(&"sbr-appnexus".to_string()));
        assert!(extracted.all_ids.contains(&"sbr-rubicon".to_string()));
        assert_eq!(
            extracted.imp_to_auction_resp_id.get("imp-1").unwrap(),
            "sar-1"
        );
        let bidders = extracted.imp_bidder_to_resp_id.get("imp-1").unwrap();
        assert_eq!(bidders.get("appnexus").unwrap(), "sbr-appnexus");
        assert_eq!(bidders.get("rubicon").unwrap(), "sbr-rubicon");
        let replace = extracted.imp_bidder_replace_imp.get("imp-1").unwrap();
        assert_eq!(replace.get("appnexus").copied(), Some(true));
        assert_eq!(replace.get("rubicon").copied(), Some(false));
    }

    #[tokio::test]
    async fn process_stored_responses_end_to_end() {
        let req = json!({
            "imp": [
                {
                    "id": "imp-1",
                    "ext": {
                        "prebid": {
                            "storedauctionresponse": { "id": "sar-1" },
                            "bidder": { "appnexus": {} },
                            "storedbidresponse": [
                                { "id": "sbr-appnexus", "bidder": "appnexus" }
                            ]
                        }
                    }
                }
            ]
        });

        let mut responses = HashMap::new();
        responses.insert("sar-1".to_string(), json!([{"seatbid": []}]));
        responses.insert("sbr-appnexus".to_string(), json!({"seat": "appnexus"}));
        let fetcher = InMemoryStoredResponsesFetcher::new(responses);

        let (imps, imp_bidder, replace) =
            process_stored_responses(&req, &fetcher).await.unwrap();

        assert!(imps.contains_key("imp-1"));
        assert!(imp_bidder
            .get("imp-1")
            .and_then(|b| b.get("appnexus"))
            .is_some());
        assert_eq!(
            replace
                .get("imp-1")
                .and_then(|m| m.get("appnexus"))
                .copied(),
            Some(true)
        );
    }

    #[tokio::test]
    async fn process_stored_responses_missing_id_returns_error() {
        let req = json!({
            "imp": [
                {
                    "id": "imp-1",
                    "ext": {
                        "prebid": {
                            "storedauctionresponse": { "id": "missing" }
                        }
                    }
                }
            ]
        });
        let fetcher = InMemoryStoredResponsesFetcher::new(HashMap::new());
        let err = process_stored_responses(&req, &fetcher).await.unwrap_err();
        assert!(matches!(err, StoredRespError::NotFound(_)));
    }
}
