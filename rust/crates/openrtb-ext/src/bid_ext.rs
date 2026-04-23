//! Prebid-specific extensions for `SeatBid.Bid.ext`.
//!
//! Mirrors `openrtb_ext/bid.go` (`ExtBid`, `ExtBidPrebid`,
//! `ExtBidPrebidCache`, `ExtBidPrebidMeta`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Targeting keys attached to the bid response (`key → value`).
pub type Targeting = HashMap<String, String>;

/// `ExtBid` — `seatbid[].bid[].ext`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtBid {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prebid: Option<ExtBidPrebid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dsa: Option<Value>,
}

/// `ExtBidPrebid` — `seatbid[].bid[].ext.prebid`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtBidPrebid {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<ExtBidPrebidCache>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dealpriority: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dealtiersatisfied: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ExtBidPrebidMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targeting: Option<Targeting>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targetbiddercode: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub bid_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passthrough: Option<Value>,
}

/// `ExtBidPrebidCache` — `seatbid[].bid[].ext.prebid.cache`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtBidPrebidCache {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bids: Option<ExtBidPrebidCacheBids>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtBidPrebidCacheBids {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(rename = "cacheId", skip_serializing_if = "Option::is_none")]
    pub cache_id: Option<String>,
}

/// `ExtBidPrebidMeta` — `seatbid[].bid[].ext.prebid.meta`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtBidPrebidMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adaptercode: Option<String>,
    #[serde(rename = "advertiserDomains", skip_serializing_if = "Option::is_none")]
    pub advertiser_domains: Option<Vec<String>>,
    #[serde(rename = "advertiserId", skip_serializing_if = "Option::is_none")]
    pub advertiser_id: Option<i64>,
    #[serde(rename = "advertiserName", skip_serializing_if = "Option::is_none")]
    pub advertiser_name: Option<String>,
    #[serde(rename = "agencyId", skip_serializing_if = "Option::is_none")]
    pub agency_id: Option<i64>,
    #[serde(rename = "agencyName", skip_serializing_if = "Option::is_none")]
    pub agency_name: Option<String>,
    #[serde(rename = "brandId", skip_serializing_if = "Option::is_none")]
    pub brand_id: Option<i64>,
    #[serde(rename = "brandName", skip_serializing_if = "Option::is_none")]
    pub brand_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dchain: Option<Value>,
    #[serde(rename = "demandSource", skip_serializing_if = "Option::is_none")]
    pub demand_source: Option<String>,
    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(rename = "networkId", skip_serializing_if = "Option::is_none")]
    pub network_id: Option<i64>,
    #[serde(rename = "networkName", skip_serializing_if = "Option::is_none")]
    pub network_name: Option<String>,
    #[serde(rename = "primaryCatId", skip_serializing_if = "Option::is_none")]
    pub primary_category_id: Option<String>,
    #[serde(rename = "rendererName", skip_serializing_if = "Option::is_none")]
    pub renderer_name: Option<String>,
    #[serde(rename = "rendererVersion", skip_serializing_if = "Option::is_none")]
    pub renderer_version: Option<String>,
    #[serde(rename = "rendererData", skip_serializing_if = "Option::is_none")]
    pub renderer_data: Option<Value>,
    #[serde(rename = "rendererUrl", skip_serializing_if = "Option::is_none")]
    pub renderer_url: Option<String>,
    #[serde(rename = "secondaryCatIds", skip_serializing_if = "Option::is_none")]
    pub secondary_category_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seat: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ext_bid() {
        let mut targeting: Targeting = HashMap::new();
        targeting.insert("hb_pb".into(), "1.50".into());
        targeting.insert("hb_bidder".into(), "appnexus".into());

        let bid = ExtBid {
            prebid: Some(ExtBidPrebid {
                cache: Some(ExtBidPrebidCache {
                    key: Some("cache-key".into()),
                    url: Some("https://cache.example/cache-key".into()),
                    bids: Some(ExtBidPrebidCacheBids {
                        url: Some("https://cache.example/cache-key".into()),
                        cache_id: Some("cache-key".into()),
                    }),
                }),
                dealpriority: Some(1),
                dealtiersatisfied: Some(true),
                meta: Some(ExtBidPrebidMeta {
                    adaptercode: Some("appnexus".into()),
                    advertiser_domains: Some(vec!["example.com".into()]),
                    media_type: Some("banner".into()),
                    ..Default::default()
                }),
                targeting: Some(targeting),
                bid_type: Some("banner".into()),
                bidid: Some("bid-1".into()),
                ..Default::default()
            }),
            dsa: None,
        };

        let s = serde_json::to_string(&bid).unwrap();
        let parsed: ExtBid = serde_json::from_str(&s).unwrap();
        assert_eq!(bid, parsed);
    }
}
