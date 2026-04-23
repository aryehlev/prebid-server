//! Stored responses handling — mirrors Go `stored_responses` package.
//!
//! Provides types and functions for managing stored auction responses
//! and stored bid responses that can be pre-configured for specific
//! impressions and bidders.

use std::collections::HashMap;

/// Map of imp ID -> stored auction response ID.
pub type ImpsWithAuctionResponseIds = HashMap<String, String>;

/// Map of imp ID -> (bidder name -> stored bid response ID).
pub type ImpBiddersWithBidResponseIds = HashMap<String, HashMap<String, String>>;

/// List of all stored response IDs to fetch.
pub type StoredResponseIds = Vec<String>;

/// Map of stored response ID -> raw JSON response body.
pub type StoredResponseIdToStoredResponse = HashMap<String, serde_json::Value>;

/// Map of bidder name -> (imp ID -> raw JSON response body).
pub type BidderImpsWithBidResponses = HashMap<String, HashMap<String, serde_json::Value>>;

/// Map of imp ID -> raw JSON response body.
pub type ImpsWithBidResponses = HashMap<String, serde_json::Value>;

/// Map of imp ID -> (bidder name -> raw JSON stored response body).
pub type ImpBidderStoredResp = HashMap<String, HashMap<String, serde_json::Value>>;

/// Map of imp ID -> (bidder name -> whether to replace imp ID).
pub type ImpBidderReplaceImpId = HashMap<String, HashMap<String, bool>>;

/// Map of bidder name -> (imp ID -> whether to replace imp ID).
pub type BidderImpReplaceImpId = HashMap<String, HashMap<String, bool>>;

/// Initialize stored bid responses: reorganize from imp-centric to bidder-centric.
/// Mirrors Go `InitStoredBidResponses`.
pub fn init_stored_bid_responses(
    stored_bid_responses: &ImpBidderStoredResp,
) -> BidderImpsWithBidResponses {
    build_stored_resp(stored_bid_responses)
}

fn build_stored_resp(
    stored_bid_responses: &ImpBidderStoredResp,
) -> BidderImpsWithBidResponses {
    let mut bidder_to_imp_to_responses = BidderImpsWithBidResponses::new();
    for (imp_id, stored_data) in stored_bid_responses {
        for (bidder_name, stored_resp) in stored_data {
            bidder_to_imp_to_responses
                .entry(bidder_name.clone())
                .or_insert_with(HashMap::new)
                .insert(imp_id.clone(), stored_resp.clone());
        }
    }
    bidder_to_imp_to_responses
}

/// Flip a map from imp_id -> bidder -> value to bidder -> imp_id -> value.
/// Mirrors Go `flipMap`.
pub fn flip_map(imp_bidder_replace: &ImpBidderReplaceImpId) -> BidderImpReplaceImpId {
    let mut flipped = BidderImpReplaceImpId::new();
    for (imp_id, imp_data) in imp_bidder_replace {
        for (bidder, replace_imp_id) in imp_data {
            flipped
                .entry(bidder.clone())
                .or_insert_with(HashMap::new)
                .insert(imp_id.clone(), *replace_imp_id);
        }
    }
    flipped
}

/// Extract stored response IDs from imp extensions.
/// Returns:
/// 1. All stored response IDs (for fetching)
/// 2. Map of imp ID -> bidder -> stored bid response ID
/// 3. Map of imp ID -> stored auction response ID
/// 4. Map of imp ID -> bidder -> whether to replace imp ID
pub fn extract_stored_responses_ids(
    imps: &[openrtb::Imp],
) -> Result<
    (
        StoredResponseIds,
        ImpBiddersWithBidResponseIds,
        ImpsWithAuctionResponseIds,
        ImpBidderReplaceImpId,
    ),
    String,
> {
    let mut all_stored_response_ids = StoredResponseIds::new();
    let mut imp_bidders_with_bid_response_ids = ImpBiddersWithBidResponseIds::new();
    let mut imp_auction_response_ids = ImpsWithAuctionResponseIds::new();
    let mut imp_bidder_replace_imp = ImpBidderReplaceImpId::new();

    for (index, imp) in imps.iter().enumerate() {
        let imp_id = &imp.id;

        // Parse imp.ext.prebid
        let prebid = match &imp.ext {
            Some(ext) => {
                let prebid_val = ext.get("prebid");
                match prebid_val {
                    Some(pv) => {
                        match serde_json::from_value::<openrtb_ext::ExtImpPrebid>(pv.clone()) {
                            Ok(p) => Some(p),
                            Err(_) => None,
                        }
                    }
                    None => None,
                }
            }
            None => None,
        };

        let prebid = match prebid {
            Some(p) => p,
            None => continue,
        };

        // Stored auction response
        if let Some(ref sar) = prebid.storedauctionresponse {
            if sar.id.is_empty() {
                return Err(format!(
                    "request.imp[{}] has ext.prebid.storedauctionresponse specified, but \"id\" field is missing",
                    index
                ));
            }
            all_stored_response_ids.push(sar.id.clone());
            imp_auction_response_ids.insert(imp_id.clone(), sar.id.clone());
        }

        // Stored bid responses
        if let Some(ref sbrs) = prebid.storedbidresponse {
            if !sbrs.is_empty() {
                let mut bidder_stored_resp_id = HashMap::new();
                let mut bidder_replace_imp_id = HashMap::new();

                for sbr in sbrs {
                    if sbr.id.is_empty() || sbr.bidder.is_empty() {
                        return Err(format!(
                            "request.imp[{}] has ext.prebid.storedbidresponse specified, but \"id\" or/and \"bidder\" fields are missing",
                            index
                        ));
                    }

                    bidder_stored_resp_id.insert(sbr.bidder.clone(), sbr.id.clone());

                    let replace_imp_id = sbr.replaceimpid.unwrap_or(true);
                    bidder_replace_imp_id.insert(sbr.bidder.clone(), replace_imp_id);

                    all_stored_response_ids.push(sbr.id.clone());
                }

                imp_bidders_with_bid_response_ids.insert(imp_id.clone(), bidder_stored_resp_id);
                imp_bidder_replace_imp.insert(imp_id.clone(), bidder_replace_imp_id);
            }
        }
    }

    Ok((
        all_stored_response_ids,
        imp_bidders_with_bid_response_ids,
        imp_auction_response_ids,
        imp_bidder_replace_imp,
    ))
}

/// Build stored response maps from fetched responses.
/// Mirrors Go `buildStoredResponsesMaps`.
pub fn build_stored_responses_maps(
    stored_responses: &StoredResponseIdToStoredResponse,
    imp_bidder_to_stored_bid_response_id: &ImpBiddersWithBidResponseIds,
    imp_id_to_resp_id: &ImpsWithAuctionResponseIds,
) -> (ImpsWithBidResponses, ImpBidderStoredResp, Vec<String>) {
    let mut errors = Vec::new();
    let mut imp_id_to_stored_resp = ImpsWithBidResponses::new();
    let mut imp_bidder_to_stored_bid_response = ImpBidderStoredResp::new();

    // Build auction response map
    for (imp_id, resp_id) in imp_id_to_resp_id {
        match stored_responses.get(resp_id) {
            Some(resp) if !resp.is_null() => {
                imp_id_to_stored_resp.insert(imp_id.clone(), resp.clone());
            }
            _ => {
                errors.push(format!(
                    "failed to fetch stored auction response for impId = {} and storedAuctionResponse id = {}",
                    imp_id, resp_id
                ));
            }
        }
    }

    // Build bid response map
    for (imp_id, bidder_stored_resp) in imp_bidder_to_stored_bid_response_id {
        let mut bidder_stored_responses = HashMap::new();
        for (bidder_name, id) in bidder_stored_resp {
            match stored_responses.get(id) {
                Some(resp) if !resp.is_null() => {
                    bidder_stored_responses.insert(bidder_name.clone(), resp.clone());
                }
                _ => {
                    errors.push(format!(
                        "failed to fetch stored bid response for impId = {}, bidder = {} and storedBidResponse id = {}",
                        imp_id, bidder_name, id
                    ));
                }
            }
        }
        imp_bidder_to_stored_bid_response.insert(imp_id.clone(), bidder_stored_responses);
    }

    (imp_id_to_stored_resp, imp_bidder_to_stored_bid_response, errors)
}

/// Fetcher trait for stored requests/responses — mirrors Go `stored_requests.Fetcher`.
pub trait StoredDataFetcher: Send + Sync {
    fn fetch_requests(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, serde_json::Value>, Vec<String>>;

    fn fetch_responses(
        &self,
        ids: &[String],
    ) -> Result<StoredResponseIdToStoredResponse, Vec<String>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_stored_bid_responses() {
        let mut stored = ImpBidderStoredResp::new();
        let mut bidder_data = HashMap::new();
        bidder_data.insert("appnexus".to_string(), serde_json::json!({"bid": 1}));
        bidder_data.insert("rubicon".to_string(), serde_json::json!({"bid": 2}));
        stored.insert("imp-1".to_string(), bidder_data);

        let result = init_stored_bid_responses(&stored);
        assert!(result.contains_key("appnexus"));
        assert!(result.contains_key("rubicon"));
        assert_eq!(result["appnexus"]["imp-1"], serde_json::json!({"bid": 1}));
    }

    #[test]
    fn test_flip_map() {
        let mut input = ImpBidderReplaceImpId::new();
        let mut bidder_data = HashMap::new();
        bidder_data.insert("appnexus".to_string(), true);
        bidder_data.insert("rubicon".to_string(), false);
        input.insert("imp-1".to_string(), bidder_data);

        let result = flip_map(&input);
        assert_eq!(result["appnexus"]["imp-1"], true);
        assert_eq!(result["rubicon"]["imp-1"], false);
    }

    #[test]
    fn test_build_stored_responses_maps() {
        let mut stored_responses = StoredResponseIdToStoredResponse::new();
        stored_responses.insert("resp-1".to_string(), serde_json::json!({"seatbid": []}));
        stored_responses.insert("resp-2".to_string(), serde_json::json!({"bid": "data"}));

        let mut imp_id_to_resp = ImpsWithAuctionResponseIds::new();
        imp_id_to_resp.insert("imp-1".to_string(), "resp-1".to_string());

        let mut imp_bidder_to_stored = ImpBiddersWithBidResponseIds::new();
        let mut bidder_map = HashMap::new();
        bidder_map.insert("appnexus".to_string(), "resp-2".to_string());
        imp_bidder_to_stored.insert("imp-2".to_string(), bidder_map);

        let (auction_resps, bid_resps, errors) =
            build_stored_responses_maps(&stored_responses, &imp_bidder_to_stored, &imp_id_to_resp);

        assert!(errors.is_empty());
        assert_eq!(auction_resps["imp-1"], serde_json::json!({"seatbid": []}));
        assert_eq!(bid_resps["imp-2"]["appnexus"], serde_json::json!({"bid": "data"}));
    }

    #[test]
    fn test_build_stored_responses_maps_missing_response() {
        let stored_responses = StoredResponseIdToStoredResponse::new();
        let mut imp_id_to_resp = ImpsWithAuctionResponseIds::new();
        imp_id_to_resp.insert("imp-1".to_string(), "missing-id".to_string());

        let (_, _, errors) = build_stored_responses_maps(
            &stored_responses,
            &ImpBiddersWithBidResponseIds::new(),
            &imp_id_to_resp,
        );

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("failed to fetch"));
    }
}
