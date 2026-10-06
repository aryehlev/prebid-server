//! Native 1.2 request markup (Go `native1/request`).

use serde::{Deserialize, Serialize};

use super::{
    AdUnit, ContextSubType, ContextType, DataAssetType, EventTrackingMethod, EventType,
    ImageAssetType, Layout, PlacementType,
};
use crate::ortb::{de, Ext};

/// The native video asset is the OpenRTB 2 video object (Go `type Video = openrtb2.Video`).
pub use crate::ortb::openrtb2::Video;

/// Native markup request (§4.1). `assets` is always written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Request {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub ver: String,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub layout: Layout,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub adunit: AdUnit,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub context: ContextType,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub contextsubtype: ContextSubType,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub plcmttype: PlacementType,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub plcmtcnt: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub seq: i64,
    #[serde(deserialize_with = "de::seq")]
    pub assets: Vec<Asset>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub aurlsupport: i8,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub durlsupport: i8,
    /// `None` is Go's nil slice: a missing or `null` list. Go's native version converter adds
    /// the default 1.2 tracker only to a nil list (`versionconvertor/service.go:60-62`), so an
    /// explicit `[]` stays empty. Written like Go's `omitempty` (neither nil nor empty).
    #[serde(skip_serializing_if = "de::is_none_or_empty")]
    pub eventtrackers: Option<Vec<EventTracker>>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub privacy: i8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Request asset (§4.2). `id` is always written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Asset {
    #[serde(deserialize_with = "de::int")]
    pub id: i64,
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
    pub ext: Option<Ext>,
}

/// Title asset request (§4.3). `len` is always written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Title {
    #[serde(deserialize_with = "de::int")]
    pub len: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Image asset request (§4.4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Image {
    #[serde(skip_serializing_if = "de::is_zero")]
    pub r#type: ImageAssetType,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub w: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub wmin: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub h: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub hmin: i64,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub mimes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Data asset request (§4.6). `type` is always written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub r#type: DataAssetType,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub len: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Event tracker request (§4.7). `event` and `methods` are always written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EventTracker {
    pub event: EventType,
    #[serde(deserialize_with = "de::seq")]
    pub methods: Vec<EventTrackingMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}
