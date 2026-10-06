use serde::{Deserialize, Serialize};

use super::MarkupType;
use crate::ortb::adcom1::{
    ApiFramework, CategoryTaxonomy, CreativeAttribute, MediaCreativeSubtype, MediaRating,
    SlotPositionInPod,
};
use crate::ortb::openrtb3::NoBidReason;
use crate::ortb::{de, Ext};

/// Top-level bid response (§4.2.1). `id` is always written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BidResponse {
    #[serde(deserialize_with = "de::string")]
    pub id: String,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub seatbid: Vec<SeatBid>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub bidid: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub cur: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub customdata: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbr: Option<NoBidReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Seat bid (§4.2.2). `bid` is always written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeatBid {
    #[serde(deserialize_with = "de::seq")]
    pub bid: Vec<Bid>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub seat: String,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub group: i8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Bid (§4.2.3). `id`, `impid` and `price` are always written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bid {
    #[serde(deserialize_with = "de::string")]
    pub id: String,
    #[serde(deserialize_with = "de::string")]
    pub impid: String,
    #[serde(deserialize_with = "de::float")]
    pub price: f64,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub nurl: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub burl: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub lurl: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub adm: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub adid: String,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub adomain: Vec<String>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub bundle: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub iurl: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub cid: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub crid: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub tactic: String,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub cattax: CategoryTaxonomy,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub cat: Vec<String>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub attr: Vec<CreativeAttribute>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub apis: Vec<ApiFramework>,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub api: ApiFramework,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub protocol: MediaCreativeSubtype,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub qagmediarating: MediaRating,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub language: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub langb: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub dealid: String,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub w: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub h: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub wratio: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub hratio: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub exp: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub dur: i64,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub mtype: MarkupType,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub slotinpod: SlotPositionInPod,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}
