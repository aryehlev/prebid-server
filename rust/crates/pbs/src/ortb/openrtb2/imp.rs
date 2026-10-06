use serde::{Deserialize, Serialize};

use super::{AdInsertion, BannerAdType};
use crate::ortb::adcom1::{
    ApiFramework, AutoRefreshTrigger, CompanionType, CreativeAttribute, DeliveryMethod,
    DoohMultiplierMeasurementSourceType, ExpandableDirection, FeedType, LinearityMode,
    MediaCreativeSubtype, PlacementPosition, PlaybackCessationMode, PlaybackMethod, PodSequence,
    SlotPositionInPod, StartDelay, VideoPlacementSubtype, VideoPlcmtSubtype,
    VolumeNormalizationMode,
};
use crate::ortb::{de, Ext};

/// Impression (§3.2.4).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Imp {
    #[serde(deserialize_with = "de::string")]
    pub id: String,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub metric: Vec<Metric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<Banner>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<Video>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<Audio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native: Option<Native>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pmp: Option<Pmp>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub displaymanager: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub displaymanagerver: String,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub instl: i8,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub tagid: String,
    #[serde(deserialize_with = "de::float", skip_serializing_if = "de::is_zero")]
    pub bidfloor: f64,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub bidfloorcur: String,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub clickbrowser: Option<i8>,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub secure: Option<i8>,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub iframebuster: Vec<String>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub rwdd: i8,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub ssai: AdInsertion,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub exp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qty: Option<Qty>,
    #[serde(deserialize_with = "de::float", skip_serializing_if = "de::is_zero")]
    pub dt: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh: Option<Refresh>,
    // Go keeps `"ext":null` as the bytes `null`; adapters branch on `len(imp.Ext) > 0`.
    #[serde(
        default,
        deserialize_with = "de::opt_ext_keep_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub ext: Option<Ext>,
}

/// Metric (§3.2.5). `type` is always written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Metric {
    #[serde(deserialize_with = "de::string")]
    pub r#type: String,
    #[serde(deserialize_with = "de::float", skip_serializing_if = "de::is_zero")]
    pub value: f64,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub vendor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Banner (§3.2.6).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Banner {
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub format: Vec<Format>,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub w: Option<i64>,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub h: Option<i64>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub wmax: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub hmax: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub wmin: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub hmin: i64,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub btype: Vec<BannerAdType>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub battr: Vec<CreativeAttribute>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<PlacementPosition>,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub mimes: Vec<String>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub topframe: i8,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub expdir: Vec<ExpandableDirection>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub api: Vec<ApiFramework>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub id: String,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub vcm: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Format (§3.2.10).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Format {
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub w: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub h: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub wratio: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub hratio: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub wmin: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Video (§3.2.7). `mimes` is always written. Also the native 1.x request video asset.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Video {
    // Go `MIMEs []string` is `json:"mimes"` with no omitempty: nil writes `null`, `[]` writes `[]`.
    #[serde(deserialize_with = "de::opt_strings")]
    pub mimes: Option<Vec<String>>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub minduration: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub maxduration: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startdelay: Option<StartDelay>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub maxseq: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub poddur: i64,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub protocols: Vec<MediaCreativeSubtype>,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub protocol: MediaCreativeSubtype,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub w: Option<i64>,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub h: Option<i64>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub podid: String,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub podseq: PodSequence,
    #[serde(deserialize_with = "de::ints", skip_serializing_if = "Vec::is_empty")]
    pub rqddurs: Vec<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<VideoPlacementSubtype>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plcmt: Option<VideoPlcmtSubtype>,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub linearity: LinearityMode,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub skip: Option<i8>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub skipmin: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub skipafter: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub sequence: i8,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub slotinpod: SlotPositionInPod,
    #[serde(deserialize_with = "de::float", skip_serializing_if = "de::is_zero")]
    pub mincpmpersec: f64,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub battr: Vec<CreativeAttribute>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub maxextended: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub minbitrate: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub maxbitrate: i64,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub boxingallowed: Option<i8>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub playbackmethod: Vec<PlaybackMethod>,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub playbackend: PlaybackCessationMode,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub delivery: Vec<DeliveryMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<PlacementPosition>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub companionad: Vec<Banner>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub api: Vec<ApiFramework>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub companiontype: Vec<CompanionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Audio (§3.2.8). `mimes` is always written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Audio {
    // Go `MIMEs []string` is `json:"mimes"` with no omitempty: nil writes `null`, `[]` writes `[]`.
    #[serde(deserialize_with = "de::opt_strings")]
    pub mimes: Option<Vec<String>>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub minduration: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub maxduration: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub poddur: i64,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub protocols: Vec<MediaCreativeSubtype>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startdelay: Option<StartDelay>,
    #[serde(deserialize_with = "de::ints", skip_serializing_if = "Vec::is_empty")]
    pub rqddurs: Vec<i64>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub podid: String,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub podseq: PodSequence,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub sequence: i64,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub slotinpod: SlotPositionInPod,
    #[serde(deserialize_with = "de::float", skip_serializing_if = "de::is_zero")]
    pub mincpmpersec: f64,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub battr: Vec<CreativeAttribute>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub maxextended: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub minbitrate: i64,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub maxbitrate: i64,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub delivery: Vec<DeliveryMethod>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub companionad: Vec<Banner>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub api: Vec<ApiFramework>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub companiontype: Vec<CompanionType>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub maxseq: i64,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub feed: FeedType,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub stitched: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvol: Option<VolumeNormalizationMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Native (§3.2.9). `request` is the native 1.x request as a JSON string
/// (see [`crate::ortb::native1::request::Request`]); `requestobj` is the object form.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Native {
    // Go `Native.Request` is `json:"request"` with no omitempty: an empty request is written as "".
    #[serde(deserialize_with = "de::string")]
    pub request: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requestobj: Option<Ext>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub ver: String,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub api: Vec<ApiFramework>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub battr: Vec<CreativeAttribute>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Private marketplace (§3.2.11).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pmp {
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub private_auction: i8,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub deals: Vec<Deal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Deal (§3.2.12). `id` is always written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Deal {
    #[serde(deserialize_with = "de::string")]
    pub id: String,
    #[serde(deserialize_with = "de::float", skip_serializing_if = "de::is_zero")]
    pub bidfloor: f64,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub bidfloorcur: String,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub at: i64,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub wseat: Vec<String>,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub wadomain: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// DOOH impression multiplier (§3.2.31).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Qty {
    #[serde(deserialize_with = "de::float", skip_serializing_if = "de::is_zero")]
    pub multiplier: f64,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub sourcetype: DoohMultiplierMeasurementSourceType,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub vendor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Auto-refresh information (§3.2.33).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Refresh {
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub refsettings: Vec<RefSettings>,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Auto-refresh settings (§3.2.34).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RefSettings {
    #[serde(skip_serializing_if = "de::is_zero")]
    pub reftype: AutoRefreshTrigger,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub minint: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}
