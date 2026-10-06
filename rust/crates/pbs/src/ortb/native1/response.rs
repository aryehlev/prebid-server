//! Native 1.2 response markup (Go `native1/response`).

use serde::{Deserialize, Serialize};

use super::{DataAssetType, EventTrackingMethod, EventType, ImageAssetType};
use crate::ortb::{de, Ext};

/// Native markup response (§5.1). `link` is always written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Response {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub ver: String,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<Asset>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub assetsurl: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub dcourl: String,
    #[serde(deserialize_with = "de::null_default")]
    pub link: Link,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub imptrackers: Vec<String>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub jstracker: String,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub eventtrackers: Vec<EventTracker>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub privacy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Response asset (§5.2). `id` is a pointer in Go: `Some(0)` is written, `None` is not.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Asset {
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<i64>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub required: i8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<Title>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub img: Option<Image>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<Video>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Data>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Title asset (§5.3). `text` is always written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Title {
    #[serde(deserialize_with = "de::string")]
    pub text: String,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub len: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Image asset (§5.4). `url` is always written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Image {
    #[serde(skip_serializing_if = "de::is_zero")]
    pub r#type: ImageAssetType,
    #[serde(deserialize_with = "de::string")]
    pub url: String,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub w: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub h: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Data asset (§5.5). `value` is always written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    #[serde(skip_serializing_if = "de::is_zero")]
    pub r#type: DataAssetType,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub len: i64,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub label: String,
    #[serde(deserialize_with = "de::string")]
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Video asset (§5.6). `vasttag` is always written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Video {
    #[serde(deserialize_with = "de::string")]
    pub vasttag: String,
}

/// Link object (§5.7). `url` is always written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Link {
    #[serde(deserialize_with = "de::string")]
    pub url: String,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub clicktrackers: Vec<String>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub fallback: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Event tracker response (§5.8). `event` and `method` are always written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EventTracker {
    pub event: EventType,
    pub method: EventTrackingMethod,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customdata: Option<Ext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}
