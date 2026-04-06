use serde::{Deserialize, Serialize};

/// OpenRTB 2.x BidRequest
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BidRequest {
    pub id: String,
    #[serde(default)]
    pub imp: Vec<Imp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<Site>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<App>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<Device>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmax: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wseat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bseat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cur: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wlang: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badv: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bapp: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regs: Option<Regs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x BidResponse
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BidResponse {
    pub id: String,
    #[serde(default)]
    pub seatbid: Vec<SeatBid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbr: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x SeatBid
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SeatBid {
    #[serde(default)]
    pub bid: Vec<Bid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seat: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Bid
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Bid {
    pub id: String,
    pub impid: String,
    pub price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nurl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adomain: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iurl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attr: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qagmediarating: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dealid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wratio: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hratio: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lurl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tactic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtype: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dur: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Imp (Impression)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Imp {
    pub id: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displaymanager: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displaymanagerver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instl: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidfloor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidfloorcur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clickbrowser: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iframebuster: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rwdd: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssai: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<Vec<Metric>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Banner
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Banner {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<Vec<Format>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub btype: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battr: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topframe: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expdir: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vcm: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Format
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Format {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wratio: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hratio: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wmin: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Video
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Video {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minduration: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxduration: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocols: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startdelay: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<i32>,
    /// OpenRTB 2.6 video placement type (replaces `placement`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plcmt: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linearity: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipmin: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipafter: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battr: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxextended: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minbitrate: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxbitrate: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boxingallowed: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playbackmethod: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playbackend: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub companionad: Option<Vec<Banner>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub companiontype: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Audio
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Audio {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minduration: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxduration: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocols: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startdelay: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battr: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxextended: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minbitrate: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxbitrate: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub companionad: Option<Vec<Banner>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub companiontype: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxseq: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stitched: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvol: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Native
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Native {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battr: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Pmp (Private Marketplace)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Pmp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_auction: Option<i32>,
    #[serde(default)]
    pub deals: Vec<Deal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Deal
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Deal {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidfloor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidfloorcur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wseat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wadomain: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Metric
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    #[serde(rename = "type")]
    pub metric_type: String,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Site
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sectioncat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagecat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mobile: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacypolicy: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Publisher>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x App
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct App {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storeurl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sectioncat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagecat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacypolicy: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paid: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Publisher>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Publisher
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Publisher {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Content
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isrc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer: Option<Producer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prodq: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub videoquality: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contentrating: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub userrating: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qagmediarating: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livestream: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sourcerelationship: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeddable: Option<i32>,
    #[serde(default)]
    pub data: Vec<Data>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Producer
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Producer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Data {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub segment: Vec<Segment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Segment
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Device
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ua: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo: Option<Geo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dnt: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lmt: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devicetype: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub make: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub osv: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hwv: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ppi: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pxratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub js: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geofetch: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flashver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carrier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mccmnc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connectiontype: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ifa: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub didsha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub didmd5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpidsha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpidmd5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macsha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macmd5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Geo
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Geo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lat: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lon: Option<f64>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub geo_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accuracy: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lastfix: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipservice: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regionfips104: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metro: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utcoffset: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x User
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct User {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buyeruid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yob: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customdata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo: Option<Geo>,
    #[serde(default)]
    pub data: Vec<Data>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eids: Option<Vec<Eid>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.6 Extended ID (EID)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Eid {
    pub source: Option<String>,
    pub uids: Option<Vec<Uid>>,
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.6 UID (within an EID)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Uid {
    pub id: Option<String>,
    pub atype: Option<i32>,
    pub ext: Option<serde_json::Value>,
}

/// DSA Transparency entry (domain + params)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DsaTransparency {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dsaparams: Option<Vec<i32>>,
}

/// Digital Services Act (DSA) object
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Dsa {
    /// 0=not required, 1=supported, 2=required, 3=required+online platform
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dsarequired: Option<i32>,
    /// 0=publisher can't render, 1=publisher could render, 2=publisher will render
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pubrender: Option<i32>,
    /// 0=don't send, 1=optional, 2=send
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datatopub: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transparency: Option<Vec<DsaTransparency>>,
}

/// OpenRTB 2.x Regs
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Regs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coppa: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub us_privacy: Option<String>,
    /// GPP consent string (Global Privacy Platform).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpp: Option<String>,
    /// GPP Section ID list indicating which regulations apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpp_sid: Option<Vec<i8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dsa: Option<Dsa>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x Source
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fd: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pchain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schain: Option<SupplyChain>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x SupplyChain
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SupplyChain {
    pub complete: i32,
    #[serde(default)]
    pub nodes: Vec<SupplyChainNode>,
    pub ver: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// OpenRTB 2.x SupplyChainNode
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SupplyChainNode {
    pub asi: String,
    pub sid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    pub hp: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}
