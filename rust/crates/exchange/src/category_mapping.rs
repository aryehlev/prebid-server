use std::collections::HashMap;

/// Tracks a bid for deduplication purposes during category mapping.
#[derive(Debug, Clone)]
pub struct BidDedupe {
    pub bidder_name: String,
    pub bid_index: usize,
    pub bid_id: String,
    pub bid_price: String,
}

/// Represents a single bid within a seat bid for category mapping.
#[derive(Debug, Clone)]
pub struct CategoryBid {
    pub bid_id: String,
    pub category: String,
    pub duration: i32,
    pub price_bucket: String,
}

/// Result of applying category mapping and deduplication.
#[derive(Debug)]
pub struct CategoryMappingResult {
    /// Maps bid ID -> category duration key (e.g. "15.00_Sports_30s")
    pub bid_category_map: HashMap<String, String>,
    /// List of rejection messages
    pub rejections: Vec<String>,
    /// Bid IDs that should be removed
    pub removed_bid_ids: Vec<String>,
}

/// Finds the duration range bucket for a video bid.
///
/// Returns an exact match if found, otherwise the smallest range value that is
/// greater than `dur`. Returns an error if all ranges are less than `dur`.
pub fn find_duration_range(dur: i32, ranges: &[i32]) -> Result<i32, String> {
    let mut new_dur = dur;
    let mut made_selection = false;

    for &range in ranges {
        if dur > range {
            continue;
        }
        if dur == range {
            return Ok(range);
        }
        // dur < range
        if range < new_dur || !made_selection {
            new_dur = range;
            made_selection = true;
        }
    }

    if !made_selection && !ranges.is_empty() {
        return Err("bid duration exceeds maximum allowed".to_string());
    }

    Ok(new_dur)
}

/// Maps an ad server ID to its name.
///
/// 1 -> "freewheel", 2 -> "dfp".
pub fn get_primary_ad_server(id: i32) -> Result<String, String> {
    match id {
        1 => Ok("freewheel".to_string()),
        2 => Ok("dfp".to_string()),
        _ => Err(format!("Primary ad server {} not recognized", id)),
    }
}

/// Appends a formatted rejection message to the rejections list.
pub fn update_rejections(rejections: &mut Vec<String>, bid_id: &str, reason: &str) {
    let message = format!("bid rejected [bid ID: {}] reason: {}", bid_id, reason);
    rejections.push(message);
}

/// Applies category mapping with competitive exclusion / deduplication.
///
/// Takes a map of bidder name -> list of bids and duration ranges. Builds dedup
/// keys as `"{category}_{duration}s"`, handles deduplication with random
/// tie-breaking, and returns the bid-category map plus rejections.
pub fn apply_category_mapping(
    seat_bids: &HashMap<String, Vec<CategoryBid>>,
    duration_ranges: &[i32],
) -> CategoryMappingResult {
    let mut bid_category_map: HashMap<String, String> = HashMap::new();
    let mut rejections: Vec<String> = Vec::new();
    let mut removed_bid_ids: Vec<String> = Vec::new();
    let mut dedupe: HashMap<String, BidDedupe> = HashMap::new();
    let mut rng = rand::thread_rng();

    for (bidder_name, bids) in seat_bids {
        for (bid_index, bid) in bids.iter().enumerate() {
            // Reject bids with empty category
            if bid.category.is_empty() {
                update_rejections(
                    &mut rejections,
                    &bid.bid_id,
                    "Bid did not contain a category",
                );
                removed_bid_ids.push(bid.bid_id.clone());
                continue;
            }

            // Find duration bucket
            let new_dur = match find_duration_range(bid.duration, duration_ranges) {
                Ok(d) => d,
                Err(e) => {
                    update_rejections(&mut rejections, &bid.bid_id, &e);
                    removed_bid_ids.push(bid.bid_id.clone());
                    continue;
                }
            };

            let category_duration = format!("{}_{}_{}s", bid.price_bucket, bid.category, new_dur);
            let dupe_key = format!("{}_{}s", bid.category, new_dur);

            if let Some(dupe) = dedupe.get(&dupe_key) {
                let dupe_price: f64 = dupe.bid_price.parse().unwrap_or(0.0);
                let curr_price: f64 = bid.price_bucket.parse().unwrap_or(0.0);

                // Determine winner; random tie-break when prices are equal
                let current_wins = if (dupe_price - curr_price).abs() < f64::EPSILON {
                    rand::Rng::gen_bool(&mut rng, 0.5)
                } else {
                    curr_price > dupe_price
                };

                if current_wins {
                    // Current bid wins: remove the old duplicate
                    update_rejections(
                        &mut rejections,
                        &dupe.bid_id,
                        "Bid was deduplicated",
                    );
                    removed_bid_ids.push(dupe.bid_id.clone());
                    bid_category_map.remove(&dupe.bid_id);
                } else {
                    // Old bid wins: reject the current bid
                    update_rejections(
                        &mut rejections,
                        &bid.bid_id,
                        "Bid was deduplicated",
                    );
                    removed_bid_ids.push(bid.bid_id.clone());
                    continue;
                }
            }

            bid_category_map.insert(bid.bid_id.clone(), category_duration);
            dedupe.insert(
                dupe_key,
                BidDedupe {
                    bidder_name: bidder_name.clone(),
                    bid_index,
                    bid_id: bid.bid_id.clone(),
                    bid_price: bid.price_bucket.clone(),
                },
            );
        }
    }

    CategoryMappingResult {
        bid_category_map,
        rejections,
        removed_bid_ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // find_duration_range tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_find_duration_range_exact_match() {
        assert_eq!(find_duration_range(15, &[10, 15, 30]), Ok(15));
    }

    #[test]
    fn test_find_duration_range_bucketed_up() {
        // 17 should bucket up to 30 (next largest)
        assert_eq!(find_duration_range(17, &[10, 15, 30]), Ok(30));
    }

    #[test]
    fn test_find_duration_range_exceeds_maximum() {
        let result = find_duration_range(60, &[10, 15, 30]);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "bid duration exceeds maximum allowed"
        );
    }

    #[test]
    fn test_find_duration_range_empty_ranges() {
        // Empty ranges with dur=0 -> Ok(0) because made_selection is false but
        // ranges is empty, so the error branch is skipped.
        assert_eq!(find_duration_range(0, &[]), Ok(0));
    }

    #[test]
    fn test_find_duration_range_unsorted_ranges() {
        // Ranges don't have to be sorted; should still find smallest >= dur
        assert_eq!(find_duration_range(12, &[30, 10, 20, 15]), Ok(15));
    }

    #[test]
    fn test_find_duration_range_zero_duration() {
        // 0 exactly matches the first element
        assert_eq!(find_duration_range(0, &[0, 15, 30]), Ok(0));
    }

    #[test]
    fn test_find_duration_range_single_element_match() {
        assert_eq!(find_duration_range(10, &[10]), Ok(10));
    }

    #[test]
    fn test_find_duration_range_single_element_bucket_up() {
        assert_eq!(find_duration_range(5, &[10]), Ok(10));
    }

    #[test]
    fn test_find_duration_range_single_element_exceeds() {
        assert!(find_duration_range(15, &[10]).is_err());
    }

    // -------------------------------------------------------------------------
    // get_primary_ad_server tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_get_primary_ad_server_freewheel() {
        assert_eq!(get_primary_ad_server(1), Ok("freewheel".to_string()));
    }

    #[test]
    fn test_get_primary_ad_server_dfp() {
        assert_eq!(get_primary_ad_server(2), Ok("dfp".to_string()));
    }

    #[test]
    fn test_get_primary_ad_server_unknown() {
        let result = get_primary_ad_server(3);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Primary ad server 3 not recognized");
    }

    #[test]
    fn test_get_primary_ad_server_zero() {
        assert!(get_primary_ad_server(0).is_err());
    }

    #[test]
    fn test_get_primary_ad_server_negative() {
        assert!(get_primary_ad_server(-1).is_err());
    }

    // -------------------------------------------------------------------------
    // update_rejections tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_update_rejections_single() {
        let mut rejections = Vec::new();
        update_rejections(&mut rejections, "bid-1", "no category");
        assert_eq!(rejections.len(), 1);
        assert_eq!(
            rejections[0],
            "bid rejected [bid ID: bid-1] reason: no category"
        );
    }

    #[test]
    fn test_update_rejections_multiple() {
        let mut rejections = Vec::new();
        update_rejections(&mut rejections, "bid-1", "reason one");
        update_rejections(&mut rejections, "bid-2", "reason two");
        assert_eq!(rejections.len(), 2);
        assert!(rejections[0].contains("bid-1"));
        assert!(rejections[1].contains("bid-2"));
    }

    #[test]
    fn test_update_rejections_preserves_existing() {
        let mut rejections = vec!["existing entry".to_string()];
        update_rejections(&mut rejections, "bid-3", "test");
        assert_eq!(rejections.len(), 2);
        assert_eq!(rejections[0], "existing entry");
    }

    // -------------------------------------------------------------------------
    // apply_category_mapping tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_apply_category_mapping_basic() {
        let mut seat_bids = HashMap::new();
        seat_bids.insert(
            "bidder-a".to_string(),
            vec![CategoryBid {
                bid_id: "bid-1".to_string(),
                category: "Sports".to_string(),
                duration: 15,
                price_bucket: "10.00".to_string(),
            }],
        );

        let result = apply_category_mapping(&seat_bids, &[15, 30]);
        assert_eq!(result.rejections.len(), 0);
        assert_eq!(result.removed_bid_ids.len(), 0);
        assert_eq!(
            result.bid_category_map.get("bid-1").unwrap(),
            "10.00_Sports_15s"
        );
    }

    #[test]
    fn test_apply_category_mapping_rejects_empty_category() {
        let mut seat_bids = HashMap::new();
        seat_bids.insert(
            "bidder-a".to_string(),
            vec![CategoryBid {
                bid_id: "bid-1".to_string(),
                category: "".to_string(),
                duration: 15,
                price_bucket: "10.00".to_string(),
            }],
        );

        let result = apply_category_mapping(&seat_bids, &[15, 30]);
        assert_eq!(result.rejections.len(), 1);
        assert!(result.rejections[0].contains("did not contain a category"));
        assert!(result.bid_category_map.is_empty());
    }

    #[test]
    fn test_apply_category_mapping_rejects_duration_exceeds() {
        let mut seat_bids = HashMap::new();
        seat_bids.insert(
            "bidder-a".to_string(),
            vec![CategoryBid {
                bid_id: "bid-1".to_string(),
                category: "Sports".to_string(),
                duration: 60,
                price_bucket: "10.00".to_string(),
            }],
        );

        let result = apply_category_mapping(&seat_bids, &[15, 30]);
        assert_eq!(result.rejections.len(), 1);
        assert!(result.rejections[0].contains("duration exceeds maximum"));
        assert!(result.bid_category_map.is_empty());
    }

    #[test]
    fn test_apply_category_mapping_dedup_different_prices() {
        // Two bids with the same category+duration but different prices.
        // The higher-priced bid should win.
        let mut seat_bids = HashMap::new();
        seat_bids.insert(
            "bidder-a".to_string(),
            vec![
                CategoryBid {
                    bid_id: "bid-low".to_string(),
                    category: "Sports".to_string(),
                    duration: 15,
                    price_bucket: "5.00".to_string(),
                },
                CategoryBid {
                    bid_id: "bid-high".to_string(),
                    category: "Sports".to_string(),
                    duration: 15,
                    price_bucket: "10.00".to_string(),
                },
            ],
        );

        let result = apply_category_mapping(&seat_bids, &[15, 30]);

        // Exactly one bid should remain, and one should be deduplicated
        assert_eq!(result.bid_category_map.len(), 1);
        assert!(result.bid_category_map.contains_key("bid-high"));
        assert!(!result.bid_category_map.contains_key("bid-low"));

        // The low bid should have been rejected
        assert!(result.removed_bid_ids.contains(&"bid-low".to_string()));
        assert!(
            result
                .rejections
                .iter()
                .any(|r| r.contains("bid-low") && r.contains("deduplicated"))
        );
    }

    #[test]
    fn test_apply_category_mapping_no_dedup_different_categories() {
        // Two bids with different categories should not deduplicate
        let mut seat_bids = HashMap::new();
        seat_bids.insert(
            "bidder-a".to_string(),
            vec![
                CategoryBid {
                    bid_id: "bid-1".to_string(),
                    category: "Sports".to_string(),
                    duration: 15,
                    price_bucket: "10.00".to_string(),
                },
                CategoryBid {
                    bid_id: "bid-2".to_string(),
                    category: "News".to_string(),
                    duration: 15,
                    price_bucket: "10.00".to_string(),
                },
            ],
        );

        let result = apply_category_mapping(&seat_bids, &[15, 30]);
        assert_eq!(result.bid_category_map.len(), 2);
        assert_eq!(result.rejections.len(), 0);
    }

    #[test]
    fn test_apply_category_mapping_dedup_same_price_random() {
        // With equal prices, one of the two should be deduplicated (random tie-break).
        // Run multiple times; both bids should never both survive.
        let mut seat_bids = HashMap::new();
        seat_bids.insert(
            "bidder-a".to_string(),
            vec![
                CategoryBid {
                    bid_id: "bid-1".to_string(),
                    category: "Sports".to_string(),
                    duration: 15,
                    price_bucket: "10.00".to_string(),
                },
                CategoryBid {
                    bid_id: "bid-2".to_string(),
                    category: "Sports".to_string(),
                    duration: 15,
                    price_bucket: "10.00".to_string(),
                },
            ],
        );

        // Run several times to exercise randomness
        for _ in 0..10 {
            let result = apply_category_mapping(&seat_bids, &[15, 30]);
            assert_eq!(
                result.bid_category_map.len(),
                1,
                "Exactly one bid should survive dedup"
            );
            assert_eq!(result.removed_bid_ids.len(), 1);
        }
    }

    #[test]
    fn test_apply_category_mapping_multiple_bidders() {
        let mut seat_bids = HashMap::new();
        seat_bids.insert(
            "bidder-a".to_string(),
            vec![CategoryBid {
                bid_id: "bid-a1".to_string(),
                category: "Sports".to_string(),
                duration: 30,
                price_bucket: "8.00".to_string(),
            }],
        );
        seat_bids.insert(
            "bidder-b".to_string(),
            vec![CategoryBid {
                bid_id: "bid-b1".to_string(),
                category: "News".to_string(),
                duration: 15,
                price_bucket: "12.00".to_string(),
            }],
        );

        let result = apply_category_mapping(&seat_bids, &[15, 30]);
        // Different categories, no dedup
        assert_eq!(result.bid_category_map.len(), 2);
        assert_eq!(result.rejections.len(), 0);
    }
}
