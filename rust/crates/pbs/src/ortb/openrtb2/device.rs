use serde::{Deserialize, Serialize};

use crate::ortb::adcom1::{
    AgentType, ConnectionType, DeviceType, IpLocationService, LocationType, UserAgentSource,
};
use crate::ortb::{de, Ext};

/// Device (§3.2.18).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Device {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo: Option<Geo>,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub dnt: Option<i8>,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub lmt: Option<i8>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub ua: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sua: Option<UserAgent>,
    // Go matches JSON keys case-insensitively, so `IP` also reaches this field.
    #[serde(
        alias = "IP",
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub ip: String,
    // Go matches JSON keys case-insensitively, so `IPv6` also reaches this field.
    #[serde(
        alias = "IPv6",
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub ipv6: String,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub devicetype: DeviceType,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub make: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub model: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub os: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub osv: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub hwv: String,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub h: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub w: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub ppi: i64,
    #[serde(deserialize_with = "de::float", skip_serializing_if = "de::is_zero")]
    pub pxratio: f64,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub js: Option<i8>,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub geofetch: Option<i8>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub flashver: String,
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
    pub carrier: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub mccmnc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connectiontype: Option<ConnectionType>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub ifa: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub didsha1: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub didmd5: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub dpidsha1: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub dpidmd5: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub macsha1: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub macmd5: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Geo (§3.2.19).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Geo {
    #[serde(
        deserialize_with = "de::opt_float",
        skip_serializing_if = "Option::is_none"
    )]
    pub lat: Option<f64>,
    #[serde(
        deserialize_with = "de::opt_float",
        skip_serializing_if = "Option::is_none"
    )]
    pub lon: Option<f64>,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub r#type: LocationType,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub accuracy: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub lastfix: i64,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub ipservice: IpLocationService,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub country: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub region: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub regionfips104: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub metro: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub city: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub zip: String,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub utcoffset: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Structured user agent (§3.2.29).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserAgent {
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub browsers: Vec<BrandVersion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<BrandVersion>,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub mobile: Option<i8>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub architecture: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub bitness: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub model: String,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub source: UserAgentSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Brand and version (§3.2.30). `brand` is always written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrandVersion {
    #[serde(deserialize_with = "de::string")]
    pub brand: String,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub version: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// User (§3.2.20).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct User {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub id: String,
    // Go matches JSON keys case-insensitively, so `buyerUid` also reaches this field.
    #[serde(
        alias = "buyerUid",
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub buyeruid: String,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub yob: i64,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub gender: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub keywords: String,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub kwarray: Vec<String>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub customdata: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo: Option<Geo>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<super::Data>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub consent: String,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub eids: Vec<Eid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Extended identifier source (§3.2.27).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Eid {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub source: String,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub uids: Vec<Uid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Extended identifier (§3.2.28).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Uid {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub id: String,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub atype: AgentType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}
