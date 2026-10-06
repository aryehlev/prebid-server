//! Go `openrtb_ext` bid-side types the adapters fill in (`bid.go`).

use serde::{Deserialize, Serialize};

use crate::ortb::Ext;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BidType {
    #[default]
    Banner,
    Video,
    Audio,
    Native,
    /// Go `BidType("")`: an adapter that casts a raw string it did not validate (mabidder,
    /// sharethrough) leaves the zero value, and the exchange rejects it later. Written as `""`.
    #[serde(rename = "")]
    Other,
}

impl BidType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Banner => "banner",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Native => "native",
            Self::Other => "",
        }
    }

    /// Go `ParseBidType`.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "banner" => Ok(Self::Banner),
            "video" => Ok(Self::Video),
            "audio" => Ok(Self::Audio),
            "native" => Ok(Self::Native),
            other => Err(format!("invalid BidType: {other}")),
        }
    }
}

/// Go `ExtBidPrebidMeta`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ExtBidPrebidMeta {
    #[serde(rename = "adaptercode", default, skip_serializing_if = "String::is_empty")]
    pub adapter_code: String,
    #[serde(rename = "advertiserDomains", default, skip_serializing_if = "Vec::is_empty")]
    pub advertiser_domains: Vec<String>,
    #[serde(rename = "advertiserId", default, skip_serializing_if = "is_zero")]
    pub advertiser_id: i32,
    #[serde(rename = "advertiserName", default, skip_serializing_if = "String::is_empty")]
    pub advertiser_name: String,
    #[serde(rename = "agencyId", default, skip_serializing_if = "is_zero")]
    pub agency_id: i32,
    #[serde(rename = "agencyName", default, skip_serializing_if = "String::is_empty")]
    pub agency_name: String,
    #[serde(rename = "brandId", default, skip_serializing_if = "is_zero")]
    pub brand_id: i32,
    #[serde(rename = "brandName", default, skip_serializing_if = "String::is_empty")]
    pub brand_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dchain: Option<Ext>,
    #[serde(rename = "demandSource", default, skip_serializing_if = "String::is_empty")]
    pub demand_source: String,
    #[serde(rename = "mediaType", default, skip_serializing_if = "String::is_empty")]
    pub media_type: String,
    #[serde(rename = "networkId", default, skip_serializing_if = "is_zero")]
    pub network_id: i32,
    #[serde(rename = "networkName", default, skip_serializing_if = "String::is_empty")]
    pub network_name: String,
    #[serde(rename = "primaryCatId", default, skip_serializing_if = "String::is_empty")]
    pub primary_category_id: String,
    #[serde(rename = "rendererName", default, skip_serializing_if = "String::is_empty")]
    pub renderer_name: String,
    #[serde(rename = "rendererVersion", default, skip_serializing_if = "String::is_empty")]
    pub renderer_version: String,
    #[serde(rename = "rendererData", default, skip_serializing_if = "Option::is_none")]
    pub renderer_data: Option<Ext>,
    #[serde(rename = "rendererUrl", default, skip_serializing_if = "String::is_empty")]
    pub renderer_url: String,
    #[serde(rename = "secondaryCatIds", default, skip_serializing_if = "Vec::is_empty")]
    pub secondary_category_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub seat: String,
}

/// Go `ExtBidPrebidVideo`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtBidPrebidVideo {
    pub duration: i32,
    pub primary_category: String,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}
