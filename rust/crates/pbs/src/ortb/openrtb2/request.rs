use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{App, Device, Dooh, Imp, Site, User};
use crate::ortb::adcom1::CategoryTaxonomy;
use crate::ortb::{de, Ext};

/// Top-level bid request (OpenRTB 2.6 §3.2.1).
///
/// `imp` has no `omitempty` in Go; an empty list is written as `[]` (Go writes `null` for a nil slice).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BidRequest {
    #[serde(deserialize_with = "de::string")]
    pub id: String,
    #[serde(deserialize_with = "de::seq")]
    pub imp: Vec<Imp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<Site>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<App>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dooh: Option<Dooh>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<Device>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub test: i8,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub at: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub tmax: i64,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub wseat: Vec<String>,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub bseat: Vec<String>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub allimps: i8,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub cur: Vec<String>,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub wlang: Vec<String>,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub wlangb: Vec<String>,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub bcat: Vec<String>,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub cattax: CategoryTaxonomy,
    /// Shared by the buyer copies of a request and only replaced wholesale (OPT-24).
    #[serde(
        deserialize_with = "de::shared_strings",
        skip_serializing_if = "<[String]>::is_empty"
    )]
    pub badv: Arc<[String]>,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub bapp: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regs: Option<Regs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Request source / upstream decisioning (§3.2.2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Source {
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub fd: Option<i8>,
    // Go matches JSON keys case-insensitively, so `TID` also reaches this field.
    #[serde(
        alias = "TID",
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub tid: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub pchain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schain: Option<SupplyChain>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// SupplyChain object (§3.2.25). `complete`, `nodes` and `ver` are always written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SupplyChain {
    #[serde(deserialize_with = "de::int")]
    pub complete: i8,
    #[serde(deserialize_with = "de::seq")]
    pub nodes: Vec<SupplyChainNode>,
    #[serde(deserialize_with = "de::string")]
    pub ver: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// SupplyChainNode object (§3.2.26). `asi` and `sid` are always written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SupplyChainNode {
    #[serde(deserialize_with = "de::string")]
    pub asi: String,
    #[serde(deserialize_with = "de::string")]
    pub sid: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub rid: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub name: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub domain: String,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub hp: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Regulations (§3.2.3).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Regs {
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub coppa: i8,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub gdpr: Option<i8>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub us_privacy: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub gpp: String,
    #[serde(deserialize_with = "de::ints", skip_serializing_if = "Vec::is_empty")]
    pub gpp_sid: Vec<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}
