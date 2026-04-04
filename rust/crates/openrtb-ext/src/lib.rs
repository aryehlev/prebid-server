use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// BidderName is a newtype wrapper around String representing a bidder identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct BidderName(pub String);

impl BidderName {
    pub fn new(s: impl Into<String>) -> Self {
        BidderName(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BidderName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for BidderName {
    fn from(s: &str) -> Self {
        BidderName(s.to_string())
    }
}

impl From<String> for BidderName {
    fn from(s: String) -> Self {
        BidderName(s)
    }
}

// Core bidder name constants — matches Go openrtb_ext/bidders.go
pub const BIDDER_33ACROSS: &str = "33across";
pub const BIDDER_AAX: &str = "aax";
pub const BIDDER_ACEEX: &str = "aceex";
pub const BIDDER_ACUITYADS: &str = "acuityads";
pub const BIDDER_ADAGIO: &str = "adagio";
pub const BIDDER_ADELEMENT: &str = "adelement";
pub const BIDDER_ADF: &str = "adf";
pub const BIDDER_ADGENERATION: &str = "adgeneration";
pub const BIDDER_ADHESE: &str = "adhese";
pub const BIDDER_ADKERNEL: &str = "adkernel";
pub const BIDDER_ADKERNELADN: &str = "adkernelAdn";
pub const BIDDER_ADMAN: &str = "adman";
pub const BIDDER_ADMATIC: &str = "admatic";
pub const BIDDER_ADMIXER: &str = "admixer";
pub const BIDDER_ADNUNTIUS: &str = "adnuntius";
pub const BIDDER_ADOT: &str = "adot";
pub const BIDDER_ADPONE: &str = "adpone";
pub const BIDDER_ADPRIME: &str = "adprime";
pub const BIDDER_ADQUERY: &str = "adquery";
pub const BIDDER_ADRINO: &str = "adrino";
pub const BIDDER_ADTARGET: &str = "adtarget";
pub const BIDDER_ADTRGTME: &str = "adtrgtme";
pub const BIDDER_ADTONOS: &str = "adtonos";
pub const BIDDER_ADTELLIGENT: &str = "adtelligent";
pub const BIDDER_ADUPTECH: &str = "aduptech";
pub const BIDDER_ADVANGELISTS: &str = "advangelists";
pub const BIDDER_ADVERXO: &str = "adverxo";
pub const BIDDER_ADVIEW: &str = "adview";
pub const BIDDER_ADXCG: &str = "adxcg";
pub const BIDDER_ADYOULIKE: &str = "adyoulike";
pub const BIDDER_AFRONT: &str = "afront";
pub const BIDDER_AIDEM: &str = "aidem";
pub const BIDDER_AJA: &str = "aja";
pub const BIDDER_AKCELO: &str = "akcelo";
pub const BIDDER_ALGORIX: &str = "algorix";
pub const BIDDER_ALKIMI: &str = "alkimi";
pub const BIDDER_ALLIANCE_GRAVITY: &str = "alliance_gravity";
pub const BIDDER_AMX: &str = "amx";
pub const BIDDER_APACDEX: &str = "apacdex";
pub const BIDDER_APPNEXUS: &str = "appnexus";
pub const BIDDER_APPUSH: &str = "appush";
pub const BIDDER_ASO: &str = "aso";
pub const BIDDER_AUDIENCENETWORK: &str = "audienceNetwork";
pub const BIDDER_AUTOMATAD: &str = "automatad";
pub const BIDDER_AVOCET: &str = "avocet";
pub const BIDDER_AXIS: &str = "axis";
pub const BIDDER_AXONIX: &str = "axonix";
pub const BIDDER_BEACHFRONT: &str = "beachfront";
pub const BIDDER_BEINTOO: &str = "beintoo";
pub const BIDDER_BEMATTERFULL: &str = "bematterfull";
pub const BIDDER_BEOP: &str = "beop";
pub const BIDDER_BETWEEN: &str = "between";
pub const BIDDER_BEYONDMEDIA: &str = "beyondmedia";
pub const BIDDER_BIDMACHINE: &str = "bidmachine";
pub const BIDDER_BIDMATIC: &str = "bidmatic";
pub const BIDDER_BIDMYADZ: &str = "bidmyadz";
pub const BIDDER_BIDSCUBE: &str = "bidscube";
pub const BIDDER_BIDSTACK: &str = "bidstack";
pub const BIDDER_BIDTHEATRE: &str = "bidtheatre";
pub const BIDDER_BIGOAD: &str = "bigoad";
pub const BIDDER_BLASTO: &str = "blasto";
pub const BIDDER_BLIINK: &str = "bliink";
pub const BIDDER_BLIS: &str = "blis";
pub const BIDDER_BLUE: &str = "blue";
pub const BIDDER_BLUESEA: &str = "bluesea";
pub const BIDDER_BMTM: &str = "bmtm";
pub const BIDDER_BOLDWIN: &str = "boldwin";
pub const BIDDER_BOLDWIN_RAPID: &str = "boldwin_rapid";
pub const BIDDER_BRAVE: &str = "brave";
pub const BIDDER_BWX: &str = "bwx";
pub const BIDDER_CADENT_APERTURE_MX: &str = "cadent_aperture_mx";
pub const BIDDER_CCX: &str = "ccx";
pub const BIDDER_CLYDO: &str = "clydo";
pub const BIDDER_COINTRAFFIC: &str = "cointraffic";
pub const BIDDER_COINZILLA: &str = "coinzilla";
pub const BIDDER_COLOSSUS: &str = "colossus";
pub const BIDDER_COMPASS: &str = "compass";
pub const BIDDER_CONCERT: &str = "concert";
pub const BIDDER_CONNATIX: &str = "connatix";
pub const BIDDER_CONNECTAD: &str = "connectad";
pub const BIDDER_CONSUMABLE: &str = "consumable";
pub const BIDDER_CONTXTFUL: &str = "contxtful";
pub const BIDDER_CONVERSANT: &str = "conversant";
pub const BIDDER_COPPER6SSP: &str = "copper6ssp";
pub const BIDDER_CPMSTAR: &str = "cpmstar";
pub const BIDDER_CRITEO: &str = "criteo";
pub const BIDDER_CWIRE: &str = "cwire";
pub const BIDDER_DATABLOCKS: &str = "datablocks";
pub const BIDDER_DECENTERADS: &str = "decenterads";
pub const BIDDER_DEEPINTENT: &str = "deepintent";
pub const BIDDER_DEFINEMEDIA: &str = "definemedia";
pub const BIDDER_DIANOMI: &str = "dianomi";
pub const BIDDER_DISPLAYIO: &str = "displayio";
pub const BIDDER_EDGE226: &str = "edge226";
pub const BIDDER_DMX: &str = "dmx";
pub const BIDDER_DRIFTPIXEL: &str = "driftpixel";
pub const BIDDER_ELEMENTALTV: &str = "elementaltv";
pub const BIDDER_EMTV: &str = "emtv";
pub const BIDDER_EMX_DIGITAL: &str = "emx_digital";
pub const BIDDER_EPLANNING: &str = "eplanning";
pub const BIDDER_EPOM: &str = "epom";
pub const BIDDER_ESCALAX: &str = "escalax";
pub const BIDDER_EXCO: &str = "exco";
pub const BIDDER_E_VOLUTION: &str = "e_volution";
pub const BIDDER_FEEDAD: &str = "feedad";
pub const BIDDER_FLATADS: &str = "flatads";
pub const BIDDER_FLIPP: &str = "flipp";
pub const BIDDER_FREEWHEELSSP: &str = "freewheelssp";
pub const BIDDER_FWSSP: &str = "fwssp";
pub const BIDDER_FRVRADN: &str = "frvradn";
pub const BIDDER_GAMMA: &str = "gamma";
pub const BIDDER_GAMOSHI: &str = "gamoshi";
pub const BIDDER_GLOBALSUN: &str = "globalsun";
pub const BIDDER_GOLDBACH: &str = "goldbach";
pub const BIDDER_GRID: &str = "grid";
pub const BIDDER_GUMGUM: &str = "gumgum";
pub const BIDDER_HUAWEIADS: &str = "huaweiads";
pub const BIDDER_IMDS: &str = "imds";
pub const BIDDER_IMPACTIFY: &str = "impactify";
pub const BIDDER_IMPROVEDIGITAL: &str = "improvedigital";
pub const BIDDER_INFYTV: &str = "infytv";
pub const BIDDER_INMOBI: &str = "inmobi";
pub const BIDDER_INSTICATOR: &str = "insticator";
pub const BIDDER_INTENZE: &str = "intenze";
pub const BIDDER_INTERACTIVEOFFERS: &str = "interactiveoffers";
pub const BIDDER_INVIBES: &str = "invibes";
pub const BIDDER_IQX: &str = "iqx";
pub const BIDDER_IQZONE: &str = "iqzone";
pub const BIDDER_IX: &str = "ix";
pub const BIDDER_JIXIE: &str = "jixie";
pub const BIDDER_KARGO: &str = "kargo";
pub const BIDDER_KAYZEN: &str = "kayzen";
pub const BIDDER_KIDOZ: &str = "kidoz";
pub const BIDDER_KIVIADS: &str = "kiviads";
pub const BIDDER_LM_KIVIADS: &str = "lm_kiviads";
pub const BIDDER_KOBLER: &str = "kobler";
pub const BIDDER_KRUSHMEDIA: &str = "krushmedia";
pub const BIDDER_KUEEZRTB: &str = "kueezrtb";
pub const BIDDER_LEMMADIGITAL: &str = "lemmadigital";
pub const BIDDER_LIMELIGHTDIGITAL: &str = "limelightDigital";
pub const BIDDER_LOCKERDOME: &str = "lockerdome";
pub const BIDDER_LOGAN: &str = "logan";
pub const BIDDER_LOGICAD: &str = "logicad";
pub const BIDDER_LOOPME: &str = "loopme";
pub const BIDDER_LOYAL: &str = "loyal";
pub const BIDDER_LUNAMEDIA: &str = "lunamedia";
pub const BIDDER_MABIDDER: &str = "mabidder";
pub const BIDDER_MADSENSE: &str = "madsense";
pub const BIDDER_MADVERTISE: &str = "madvertise";
pub const BIDDER_MARSMEDIA: &str = "marsmedia";
pub const BIDDER_MEDIAFUSE: &str = "mediafuse";
pub const BIDDER_MEDIAGO: &str = "mediago";
pub const BIDDER_MEDIANET: &str = "medianet";
pub const BIDDER_MEDIASQUARE: &str = "mediasquare";
pub const BIDDER_MELOZEN: &str = "melozen";
pub const BIDDER_METAX: &str = "metax";
pub const BIDDER_MGID: &str = "mgid";
pub const BIDDER_MGIDX: &str = "mgidX";
pub const BIDDER_MICROSOFT: &str = "msft";
pub const BIDDER_MINUTEMEDIA: &str = "minutemedia";
pub const BIDDER_MISSENA: &str = "missena";
pub const BIDDER_MOBFOXPB: &str = "mobfoxpb";
pub const BIDDER_MOBILEFUSE: &str = "mobilefuse";
pub const BIDDER_MOBKOI: &str = "mobkoi";
pub const BIDDER_MOTORIK: &str = "motorik";
pub const BIDDER_NATIVERY: &str = "nativery";
pub const BIDDER_NATIVO: &str = "nativo";
pub const BIDDER_NEXTMILLENNIUM: &str = "nextmillennium";
pub const BIDDER_NEXX360: &str = "nexx360";
pub const BIDDER_NOBID: &str = "nobid";
pub const BIDDER_OGURY: &str = "ogury";
pub const BIDDER_OMS: &str = "oms";
pub const BIDDER_ONETAG: &str = "onetag";
pub const BIDDER_OPENWEB: &str = "openweb";
pub const BIDDER_OPENX: &str = "openx";
pub const BIDDER_OPERAADS: &str = "operaads";
pub const BIDDER_OPTIDIGITAL: &str = "optidigital";
pub const BIDDER_ORAKI: &str = "oraki";
pub const BIDDER_ORBIDDER: &str = "orbidder";
pub const BIDDER_OUTBRAIN: &str = "outbrain";
pub const BIDDER_OWNADX: &str = "ownadx";
pub const BIDDER_PANGLE: &str = "pangle";
pub const BIDDER_PGAMSSP: &str = "pgamssp";
pub const BIDDER_PLAYDIGO: &str = "playdigo";
pub const BIDDER_PUBMATIC: &str = "pubmatic";
pub const BIDDER_PUBRISE: &str = "pubrise";
pub const BIDDER_PUBNATIVE: &str = "pubnative";
pub const BIDDER_PULSEPOINT: &str = "pulsepoint";
pub const BIDDER_PWBID: &str = "pwbid";
pub const BIDDER_QT: &str = "qt";
pub const BIDDER_READPEAK: &str = "readpeak";
pub const BIDDER_REDIADS: &str = "rediads";
pub const BIDDER_RELEVANTDIGITAL: &str = "relevantdigital";
pub const BIDDER_RESETDIGITAL: &str = "resetdigital";
pub const BIDDER_REVCONTENT: &str = "revcontent";
pub const BIDDER_RICHAUDIENCE: &str = "richaudience";
pub const BIDDER_RISE: &str = "rise";
pub const BIDDER_ROULAX: &str = "roulax";
pub const BIDDER_RTBHOUSE: &str = "rtbhouse";
pub const BIDDER_RUBICON: &str = "rubicon";
pub const BIDDER_SEEDINGALLIANCE: &str = "seedingAlliance";
pub const BIDDER_SEEDTAG: &str = "seedtag";
pub const BIDDER_SA_LUNAMEDIA: &str = "sa_lunamedia";
pub const BIDDER_SHARETHROUGH: &str = "sharethrough";
pub const BIDDER_SHOWHEROES: &str = "showheroes";
pub const BIDDER_SILVERMOB: &str = "silvermob";
pub const BIDDER_SILVERPUSH: &str = "silverpush";
pub const BIDDER_SMAATO: &str = "smaato";
pub const BIDDER_SMARTADSERVER: &str = "smartadserver";
pub const BIDDER_SMARTHUB: &str = "smarthub";
pub const BIDDER_SMARTRTB: &str = "smartrtb";
pub const BIDDER_SMARTX: &str = "smartx";
pub const BIDDER_SMARTYADS: &str = "smartyads";
pub const BIDDER_SMILEWANTED: &str = "smilewanted";
pub const BIDDER_SMOOT: &str = "smoot";
pub const BIDDER_SMRTCONNECT: &str = "smrtconnect";
pub const BIDDER_SONOBI: &str = "sonobi";
pub const BIDDER_SOVRN: &str = "sovrn";
pub const BIDDER_SOVRNXSP: &str = "sovrnXsp";
pub const BIDDER_SPARTEO: &str = "sparteo";
pub const BIDDER_SSPBC: &str = "sspBC";
pub const BIDDER_STARTIO: &str = "startio";
pub const BIDDER_STROEERCORE: &str = "stroeerCore";
pub const BIDDER_TABOOLA: &str = "taboola";
pub const BIDDER_TAPPX: &str = "tappx";
pub const BIDDER_TEADS: &str = "teads";
pub const BIDDER_TELARIA: &str = "telaria";
pub const BIDDER_TEQBLAZE: &str = "teqblaze";
pub const BIDDER_THEADX: &str = "theadx";
pub const BIDDER_THETRADEDESK: &str = "thetradedesk";
pub const BIDDER_TPMN: &str = "tpmn";
pub const BIDDER_TRADPLUS: &str = "tradplus";
pub const BIDDER_TRAFFICGATE: &str = "trafficgate";
pub const BIDDER_TRIPLELIFT: &str = "triplelift";
pub const BIDDER_TRIPLELIFT_NATIVE: &str = "triplelift_native";
pub const BIDDER_TRUSTEDSTACK: &str = "trustedstack";
pub const BIDDER_TRUSTX: &str = "trustx";
pub const BIDDER_UCFUNNEL: &str = "ucfunnel";
pub const BIDDER_UNDERTONE: &str = "undertone";
pub const BIDDER_UNICORN: &str = "unicorn";
pub const BIDDER_UNRULY: &str = "unruly";
pub const BIDDER_VIDAZOO: &str = "vidazoo";
pub const BIDDER_VIDEOBYTE: &str = "videobyte";
pub const BIDDER_VIDEOHEROES: &str = "videoheroes";
pub const BIDDER_VIDOOMY: &str = "vidoomy";
pub const BIDDER_VISIBLEMEASURES: &str = "visiblemeasures";
pub const BIDDER_VISX: &str = "visx";
pub const BIDDER_VOX: &str = "vox";
pub const BIDDER_VRTCAL: &str = "vrtcal";
pub const BIDDER_VUNGLE: &str = "vungle";
pub const BIDDER_XEWORKS: &str = "xeworks";
pub const BIDDER_YAHOOADS: &str = "yahooAds";
pub const BIDDER_YANDEX: &str = "yandex";
pub const BIDDER_YEAHMOBI: &str = "yeahmobi";
pub const BIDDER_YIELDLAB: &str = "yieldlab";
pub const BIDDER_YIELDMO: &str = "yieldmo";
pub const BIDDER_YIELDONE: &str = "yieldone";
pub const BIDDER_ZENTOTEM: &str = "zentotem";
pub const BIDDER_ZEROCLICKFRAUD: &str = "zeroclickfraud";
pub const BIDDER_ZETAGLOBALSSP: &str = "zeta_global_ssp";
pub const BIDDER_ZMATICOO: &str = "zmaticoo";

// Reserved bidder names
pub const BIDDER_RESERVED_ALL: &str = "all";
pub const BIDDER_RESERVED_CONTEXT: &str = "context";
pub const BIDDER_RESERVED_DATA: &str = "data";
pub const BIDDER_RESERVED_GENERAL: &str = "general";
pub const BIDDER_RESERVED_GPID: &str = "gpid";
pub const BIDDER_RESERVED_PREBID: &str = "prebid";
pub const BIDDER_RESERVED_SKADN: &str = "skadn";
pub const BIDDER_RESERVED_TID: &str = "tid";
pub const BIDDER_RESERVED_AE: &str = "ae";
pub const BIDDER_RESERVED_IGS: &str = "igs";

/// BidType describes the type of a bid (banner, video, audio, native)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BidType {
    Banner,
    Video,
    Audio,
    Native,
}

impl Default for BidType {
    fn default() -> Self {
        BidType::Banner
    }
}

impl std::fmt::Display for BidType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BidType::Banner => write!(f, "banner"),
            BidType::Video => write!(f, "video"),
            BidType::Audio => write!(f, "audio"),
            BidType::Native => write!(f, "native"),
        }
    }
}

/// ExtBidPrebid is the prebid extension on a bid response bid object
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtBidPrebid {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<ExtBidPrebidCache>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "dealpriority")]
    pub deal_priority: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "dealtiersatisfied")]
    pub deal_tier_satisfied: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ExtBidPrebidMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targeting: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "targetbiddercode")]
    pub target_bidder_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    pub bid_type: Option<BidType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<ExtBidPrebidVideo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<ExtBidPrebidEvents>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passthrough: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floors: Option<ExtBidPrebidFloors>,
}

/// ExtBidPrebidCache defines the cache information in bid ext
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtBidPrebidCache {
    pub key: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bids: Option<ExtBidPrebidCacheBids>,
}

/// ExtBidPrebidCacheBids holds cache bid information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExtBidPrebidCacheBids {
    pub url: String,
    pub cache_id: String,
}

/// ExtBidPrebidFloors defines floor information on a bid
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtBidPrebidFloors {
    #[serde(skip_serializing_if = "Option::is_none", rename = "floorRule")]
    pub floor_rule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "floorRuleValue")]
    pub floor_rule_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "floorValue")]
    pub floor_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "floorCurrency")]
    pub floor_currency: Option<String>,
}

/// ExtBidPrebidMeta defines the meta information in bid ext
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtBidPrebidMeta {
    #[serde(skip_serializing_if = "Option::is_none", rename = "adaptercode")]
    pub adapter_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "advertiserDomains")]
    pub advertiser_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "advertiserId")]
    pub advertiser_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "advertiserName")]
    pub advertiser_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "agencyId")]
    pub agency_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "agencyName")]
    pub agency_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "brandId")]
    pub brand_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "brandName")]
    pub brand_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dchain: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "demandSource")]
    pub demand_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "mediaType")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "networkId")]
    pub network_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "networkName")]
    pub network_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "primaryCatId")]
    pub primary_category_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "rendererName")]
    pub renderer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "rendererVersion")]
    pub renderer_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "rendererData")]
    pub renderer_data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "rendererUrl")]
    pub renderer_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "secondaryCatIds")]
    pub secondary_category_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seat: Option<String>,
}

/// ExtBidPrebidVideo defines the video information in bid ext
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtBidPrebidVideo {
    pub duration: i32,
    pub primary_category: String,
}

/// ExtBidPrebidEvents defines event URLs in bid ext
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtBidPrebidEvents {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imp: Option<String>,
}

/// ExtBidPrebidVastXml holds VAST XML data (used for some video adapters)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtBidPrebidVastXml {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// ExtBidderMessage defines an error/warning message from a bidder
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtBidderMessage {
    pub code: i32,
    pub message: String,
}

/// FledgeAuctionConfig holds configuration for a FLEDGE auction
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FledgeAuctionConfig {
    pub impid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    pub config: serde_json::Value,
}

/// NonBidObject is a subset of Bid with custom fields for non-bid tracking
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NonBidObject {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adomain: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dealid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dur: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "origbidcpm")]
    pub orig_bid_cpm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "origbidcur")]
    pub orig_bid_cur: Option<String>,
}

/// NonBidExt is the ext object for NonBid
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NonBidExt {
    pub prebid: ExtResponseNonBidPrebid,
}

/// ExtResponseNonBidPrebid wraps the NonBidObject
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtResponseNonBidPrebid {
    pub bid: NonBidObject,
}

/// NonBid represents a non-bid reason for a given impression
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NonBid {
    pub impid: String,
    pub statuscode: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<NonBidExt>,
}

/// SeatNonBid is a collection of NonBid objects with seat information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SeatNonBid {
    pub nonbid: Vec<NonBid>,
    pub seat: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// ExtHttpCall defines debug http call information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtHttpCall {
    pub uri: String,
    #[serde(rename = "requestbody")]
    pub request_body: String,
    #[serde(rename = "requestheaders")]
    pub request_headers: std::collections::HashMap<String, Vec<String>>,
    #[serde(rename = "responsebody")]
    pub response_body: String,
    pub status: i32,
}

/// Generic bidder ext wrapper used in imp.ext
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpBidder {
    pub bidder: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prebid: Option<serde_json::Value>,
}

/// MultiBid describes multi-bid configuration for a bidder from req.ext.prebid.multibid
#[derive(Debug, Deserialize, Clone)]
pub struct MultiBid {
    /// Single bidder name (mutually exclusive with `bidders`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidder: Option<String>,
    /// Multiple bidder names (mutually exclusive with `bidder`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidders: Option<Vec<String>>,
    /// Maximum number of bids allowed from this bidder per imp
    #[serde(rename = "maxBids")]
    pub max_bids: Option<u32>,
    /// Prefix used to set targeting keys for additional bids
    #[serde(rename = "targetBidderCodePrefix", skip_serializing_if = "Option::is_none")]
    pub target_bidder_code_prefix: Option<String>,
}

/// ExtIncludeBrandCategory describes the includebrandcategory targeting option.
/// When present in req.ext.prebid.targeting, competitive exclusion (category mapping) is enabled.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ExtIncludeBrandCategory {
    /// 1 = FreeWheel, 2 = DFP
    #[serde(rename = "primaryAdServer")]
    pub primary_ad_server: Option<i32>,
    pub publisher: Option<String>,
    #[serde(rename = "withCategory")]
    pub with_category: Option<bool>,
    #[serde(rename = "translateCategories")]
    pub translate_categories: Option<bool>,
}

/// DealTier describes minimum deal tier configuration for an imp.pmp.deal extension.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct DealTier {
    pub prefix: Option<String>,
    #[serde(rename = "minDealTier")]
    pub min_deal_tier: Option<i32>,
}

/// BidAdjustmentRule holds the new-style bid adjustment rules from req.ext.prebid.bidadjustments.
/// The `bidders` map is keyed by bidder name, then by deal ID (or "*" for any deal),
/// and contains a list of Adjustment rules.
#[derive(Debug, Deserialize, Clone)]
pub struct BidAdjustmentRule {
    pub bidders: Option<HashMap<String, HashMap<String, Vec<Adjustment>>>>,
}

/// Adjustment is a single bid adjustment entry within a BidAdjustmentRule.
#[derive(Debug, Deserialize, Clone)]
pub struct Adjustment {
    /// "multiplier", "static", or "cpm"
    #[serde(rename = "adjtype")]
    pub adj_type: String,
    pub value: f64,
    pub currency: Option<String>,
}

// ── Cache extension structs ──────────────────────────────────────────────────

/// ExtRequestPrebidCache defines caching options for prebid request
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ExtRequestPrebidCache {
    pub bids: Option<ExtRequestPrebidCacheBids>,
    pub vastxml: Option<ExtRequestPrebidCacheVAST>,
    pub winningonly: Option<bool>,
}

/// ExtRequestPrebidCacheBids defines bid caching options
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ExtRequestPrebidCacheBids {
    pub ttl_seconds: Option<i64>,
    pub return_creative: Option<bool>,
}

/// ExtRequestPrebidCacheVAST defines VAST XML caching options
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ExtRequestPrebidCacheVAST {
    pub ttl_seconds: Option<i64>,
    pub return_creative: Option<bool>,
}

// ── Targeting extension ──────────────────────────────────────────────────────

/// ExtRequestTargeting describes targeting options from req.ext.prebid.targeting
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ExtRequestTargeting {
    pub pricegranularity: Option<serde_json::Value>,
    pub mediatypepricegranularity: Option<serde_json::Value>,
    pub currency: Option<String>,
    pub includebrandcategory: Option<ExtIncludeBrandCategory>,
    pub includeformat: Option<bool>,
    pub durationrangeinmssupport: Option<bool>,
    pub preferdeals: Option<bool>,
    pub includewinners: Option<bool>,
    pub includebidderkeys: Option<bool>,
    pub truncateattrvalue: Option<i32>,
}

// ── Response prebid extension structs ────────────────────────────────────────

/// ExtResponsePrebid holds prebid-specific fields in the response ext
#[derive(Debug, Serialize, Clone, Default)]
pub struct ExtResponsePrebid {
    pub auctiontimestamp: Option<u64>,
    pub passthrough: Option<serde_json::Value>,
    pub seatnonbid: Option<Vec<SeatNonBid>>,
    pub timing: Option<ExtResponseTiming>,
    pub errors: Option<std::collections::HashMap<String, Vec<ExtBidderMessage>>>,
}

/// ExtResponseTiming holds timing information in the response ext
#[derive(Debug, Serialize, Clone, Default)]
pub struct ExtResponseTiming {
    #[serde(rename = "respondedin")]
    pub responded_in: u64,
}

// ── Channel / SDK extension structs ─────────────────────────────────────────

/// ExtRequestPrebidChannel describes the channel (name/version) for a request
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ExtRequestPrebidChannel {
    pub name: String,
    pub version: String,
}

/// ExtRequestSdk describes the SDK used to generate the request
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ExtRequestSdk {
    pub renderers: Option<Vec<ExtRequestSdkRenderer>>,
    pub source: Option<String>,
    pub version: Option<String>,
}

/// ExtRequestSdkRenderer describes a renderer registered by the SDK
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ExtRequestSdkRenderer {
    pub name: String,
    pub version: Option<String>,
    pub data: Option<serde_json::Value>,
}

// ── Bidder name constructor functions ────────────────────────────────────────
// Each function returns a BidderName with the canonical string value from Go.

pub fn bidder_33across() -> BidderName { BidderName::new(BIDDER_33ACROSS) }
pub fn bidder_aax() -> BidderName { BidderName::new(BIDDER_AAX) }
pub fn bidder_aceex() -> BidderName { BidderName::new(BIDDER_ACEEX) }
pub fn bidder_acuityads() -> BidderName { BidderName::new(BIDDER_ACUITYADS) }
pub fn bidder_adagio() -> BidderName { BidderName::new(BIDDER_ADAGIO) }
pub fn bidder_adelement() -> BidderName { BidderName::new(BIDDER_ADELEMENT) }
pub fn bidder_adf() -> BidderName { BidderName::new(BIDDER_ADF) }
pub fn bidder_adgeneration() -> BidderName { BidderName::new(BIDDER_ADGENERATION) }
pub fn bidder_adhese() -> BidderName { BidderName::new(BIDDER_ADHESE) }
pub fn bidder_adkernel() -> BidderName { BidderName::new(BIDDER_ADKERNEL) }
pub fn bidder_adkerneladn() -> BidderName { BidderName::new(BIDDER_ADKERNELADN) }
pub fn bidder_adman() -> BidderName { BidderName::new(BIDDER_ADMAN) }
pub fn bidder_admatic() -> BidderName { BidderName::new(BIDDER_ADMATIC) }
pub fn bidder_admixer() -> BidderName { BidderName::new(BIDDER_ADMIXER) }
pub fn bidder_adnuntius() -> BidderName { BidderName::new(BIDDER_ADNUNTIUS) }
pub fn bidder_adot() -> BidderName { BidderName::new(BIDDER_ADOT) }
pub fn bidder_adpone() -> BidderName { BidderName::new(BIDDER_ADPONE) }
pub fn bidder_adprime() -> BidderName { BidderName::new(BIDDER_ADPRIME) }
pub fn bidder_adquery() -> BidderName { BidderName::new(BIDDER_ADQUERY) }
pub fn bidder_adrino() -> BidderName { BidderName::new(BIDDER_ADRINO) }
pub fn bidder_adtarget() -> BidderName { BidderName::new(BIDDER_ADTARGET) }
pub fn bidder_adtrgtme() -> BidderName { BidderName::new(BIDDER_ADTRGTME) }
pub fn bidder_adtonos() -> BidderName { BidderName::new(BIDDER_ADTONOS) }
pub fn bidder_adtelligent() -> BidderName { BidderName::new(BIDDER_ADTELLIGENT) }
pub fn bidder_aduptech() -> BidderName { BidderName::new(BIDDER_ADUPTECH) }
pub fn bidder_advangelists() -> BidderName { BidderName::new(BIDDER_ADVANGELISTS) }
pub fn bidder_adverxo() -> BidderName { BidderName::new(BIDDER_ADVERXO) }
pub fn bidder_adview() -> BidderName { BidderName::new(BIDDER_ADVIEW) }
pub fn bidder_adxcg() -> BidderName { BidderName::new(BIDDER_ADXCG) }
pub fn bidder_adyoulike() -> BidderName { BidderName::new(BIDDER_ADYOULIKE) }
pub fn bidder_afront() -> BidderName { BidderName::new(BIDDER_AFRONT) }
pub fn bidder_aidem() -> BidderName { BidderName::new(BIDDER_AIDEM) }
pub fn bidder_aja() -> BidderName { BidderName::new(BIDDER_AJA) }
pub fn bidder_akcelo() -> BidderName { BidderName::new(BIDDER_AKCELO) }
pub fn bidder_algorix() -> BidderName { BidderName::new(BIDDER_ALGORIX) }
pub fn bidder_alkimi() -> BidderName { BidderName::new(BIDDER_ALKIMI) }
pub fn bidder_alliance_gravity() -> BidderName { BidderName::new(BIDDER_ALLIANCE_GRAVITY) }
pub fn bidder_amx() -> BidderName { BidderName::new(BIDDER_AMX) }
pub fn bidder_apacdex() -> BidderName { BidderName::new(BIDDER_APACDEX) }
pub fn bidder_appnexus() -> BidderName { BidderName::new(BIDDER_APPNEXUS) }
pub fn bidder_appush() -> BidderName { BidderName::new(BIDDER_APPUSH) }
pub fn bidder_aso() -> BidderName { BidderName::new(BIDDER_ASO) }
pub fn bidder_audiencenetwork() -> BidderName { BidderName::new(BIDDER_AUDIENCENETWORK) }
pub fn bidder_automatad() -> BidderName { BidderName::new(BIDDER_AUTOMATAD) }
pub fn bidder_avocet() -> BidderName { BidderName::new(BIDDER_AVOCET) }
pub fn bidder_axis() -> BidderName { BidderName::new(BIDDER_AXIS) }
pub fn bidder_axonix() -> BidderName { BidderName::new(BIDDER_AXONIX) }
pub fn bidder_beachfront() -> BidderName { BidderName::new(BIDDER_BEACHFRONT) }
pub fn bidder_beintoo() -> BidderName { BidderName::new(BIDDER_BEINTOO) }
pub fn bidder_bematterfull() -> BidderName { BidderName::new(BIDDER_BEMATTERFULL) }
pub fn bidder_beop() -> BidderName { BidderName::new(BIDDER_BEOP) }
pub fn bidder_between() -> BidderName { BidderName::new(BIDDER_BETWEEN) }
pub fn bidder_beyondmedia() -> BidderName { BidderName::new(BIDDER_BEYONDMEDIA) }
pub fn bidder_bidmachine() -> BidderName { BidderName::new(BIDDER_BIDMACHINE) }
pub fn bidder_bidmatic() -> BidderName { BidderName::new(BIDDER_BIDMATIC) }
pub fn bidder_bidmyadz() -> BidderName { BidderName::new(BIDDER_BIDMYADZ) }
pub fn bidder_bidscube() -> BidderName { BidderName::new(BIDDER_BIDSCUBE) }
pub fn bidder_bidstack() -> BidderName { BidderName::new(BIDDER_BIDSTACK) }
pub fn bidder_bidtheatre() -> BidderName { BidderName::new(BIDDER_BIDTHEATRE) }
pub fn bidder_bigoad() -> BidderName { BidderName::new(BIDDER_BIGOAD) }
pub fn bidder_blasto() -> BidderName { BidderName::new(BIDDER_BLASTO) }
pub fn bidder_bliink() -> BidderName { BidderName::new(BIDDER_BLIINK) }
pub fn bidder_blis() -> BidderName { BidderName::new(BIDDER_BLIS) }
pub fn bidder_blue() -> BidderName { BidderName::new(BIDDER_BLUE) }
pub fn bidder_bluesea() -> BidderName { BidderName::new(BIDDER_BLUESEA) }
pub fn bidder_bmtm() -> BidderName { BidderName::new(BIDDER_BMTM) }
pub fn bidder_boldwin() -> BidderName { BidderName::new(BIDDER_BOLDWIN) }
pub fn bidder_boldwin_rapid() -> BidderName { BidderName::new(BIDDER_BOLDWIN_RAPID) }
pub fn bidder_brave() -> BidderName { BidderName::new(BIDDER_BRAVE) }
pub fn bidder_bwx() -> BidderName { BidderName::new(BIDDER_BWX) }
pub fn bidder_cadent_aperture_mx() -> BidderName { BidderName::new(BIDDER_CADENT_APERTURE_MX) }
pub fn bidder_ccx() -> BidderName { BidderName::new(BIDDER_CCX) }
pub fn bidder_clydo() -> BidderName { BidderName::new(BIDDER_CLYDO) }
pub fn bidder_cointraffic() -> BidderName { BidderName::new(BIDDER_COINTRAFFIC) }
pub fn bidder_coinzilla() -> BidderName { BidderName::new(BIDDER_COINZILLA) }
pub fn bidder_colossus() -> BidderName { BidderName::new(BIDDER_COLOSSUS) }
pub fn bidder_compass() -> BidderName { BidderName::new(BIDDER_COMPASS) }
pub fn bidder_concert() -> BidderName { BidderName::new(BIDDER_CONCERT) }
pub fn bidder_connatix() -> BidderName { BidderName::new(BIDDER_CONNATIX) }
pub fn bidder_connectad() -> BidderName { BidderName::new(BIDDER_CONNECTAD) }
pub fn bidder_consumable() -> BidderName { BidderName::new(BIDDER_CONSUMABLE) }
pub fn bidder_contxtful() -> BidderName { BidderName::new(BIDDER_CONTXTFUL) }
pub fn bidder_conversant() -> BidderName { BidderName::new(BIDDER_CONVERSANT) }
pub fn bidder_copper6ssp() -> BidderName { BidderName::new(BIDDER_COPPER6SSP) }
pub fn bidder_cpmstar() -> BidderName { BidderName::new(BIDDER_CPMSTAR) }
pub fn bidder_criteo() -> BidderName { BidderName::new(BIDDER_CRITEO) }
pub fn bidder_cwire() -> BidderName { BidderName::new(BIDDER_CWIRE) }
pub fn bidder_datablocks() -> BidderName { BidderName::new(BIDDER_DATABLOCKS) }
pub fn bidder_decenterads() -> BidderName { BidderName::new(BIDDER_DECENTERADS) }
pub fn bidder_deepintent() -> BidderName { BidderName::new(BIDDER_DEEPINTENT) }
pub fn bidder_definemedia() -> BidderName { BidderName::new(BIDDER_DEFINEMEDIA) }
pub fn bidder_dianomi() -> BidderName { BidderName::new(BIDDER_DIANOMI) }
pub fn bidder_displayio() -> BidderName { BidderName::new(BIDDER_DISPLAYIO) }
pub fn bidder_edge226() -> BidderName { BidderName::new(BIDDER_EDGE226) }
pub fn bidder_dmx() -> BidderName { BidderName::new(BIDDER_DMX) }
pub fn bidder_driftpixel() -> BidderName { BidderName::new(BIDDER_DRIFTPIXEL) }
pub fn bidder_elementaltv() -> BidderName { BidderName::new(BIDDER_ELEMENTALTV) }
pub fn bidder_emtv() -> BidderName { BidderName::new(BIDDER_EMTV) }
pub fn bidder_emx_digital() -> BidderName { BidderName::new(BIDDER_EMX_DIGITAL) }
pub fn bidder_eplanning() -> BidderName { BidderName::new(BIDDER_EPLANNING) }
pub fn bidder_epom() -> BidderName { BidderName::new(BIDDER_EPOM) }
pub fn bidder_escalax() -> BidderName { BidderName::new(BIDDER_ESCALAX) }
pub fn bidder_exco() -> BidderName { BidderName::new(BIDDER_EXCO) }
pub fn bidder_e_volution() -> BidderName { BidderName::new(BIDDER_E_VOLUTION) }
pub fn bidder_feedad() -> BidderName { BidderName::new(BIDDER_FEEDAD) }
pub fn bidder_flatads() -> BidderName { BidderName::new(BIDDER_FLATADS) }
pub fn bidder_flipp() -> BidderName { BidderName::new(BIDDER_FLIPP) }
pub fn bidder_freewheelssp() -> BidderName { BidderName::new(BIDDER_FREEWHEELSSP) }
pub fn bidder_fwssp() -> BidderName { BidderName::new(BIDDER_FWSSP) }
pub fn bidder_frvradn() -> BidderName { BidderName::new(BIDDER_FRVRADN) }
pub fn bidder_gamma() -> BidderName { BidderName::new(BIDDER_GAMMA) }
pub fn bidder_gamoshi() -> BidderName { BidderName::new(BIDDER_GAMOSHI) }
pub fn bidder_globalsun() -> BidderName { BidderName::new(BIDDER_GLOBALSUN) }
pub fn bidder_goldbach() -> BidderName { BidderName::new(BIDDER_GOLDBACH) }
pub fn bidder_grid() -> BidderName { BidderName::new(BIDDER_GRID) }
pub fn bidder_gumgum() -> BidderName { BidderName::new(BIDDER_GUMGUM) }
pub fn bidder_huaweiads() -> BidderName { BidderName::new(BIDDER_HUAWEIADS) }
pub fn bidder_imds() -> BidderName { BidderName::new(BIDDER_IMDS) }
pub fn bidder_impactify() -> BidderName { BidderName::new(BIDDER_IMPACTIFY) }
pub fn bidder_improvedigital() -> BidderName { BidderName::new(BIDDER_IMPROVEDIGITAL) }
pub fn bidder_infytv() -> BidderName { BidderName::new(BIDDER_INFYTV) }
pub fn bidder_inmobi() -> BidderName { BidderName::new(BIDDER_INMOBI) }
pub fn bidder_insticator() -> BidderName { BidderName::new(BIDDER_INSTICATOR) }
pub fn bidder_intenze() -> BidderName { BidderName::new(BIDDER_INTENZE) }
pub fn bidder_interactiveoffers() -> BidderName { BidderName::new(BIDDER_INTERACTIVEOFFERS) }
pub fn bidder_invibes() -> BidderName { BidderName::new(BIDDER_INVIBES) }
pub fn bidder_iqx() -> BidderName { BidderName::new(BIDDER_IQX) }
pub fn bidder_iqzone() -> BidderName { BidderName::new(BIDDER_IQZONE) }
pub fn bidder_ix() -> BidderName { BidderName::new(BIDDER_IX) }
pub fn bidder_jixie() -> BidderName { BidderName::new(BIDDER_JIXIE) }
pub fn bidder_kargo() -> BidderName { BidderName::new(BIDDER_KARGO) }
pub fn bidder_kayzen() -> BidderName { BidderName::new(BIDDER_KAYZEN) }
pub fn bidder_kidoz() -> BidderName { BidderName::new(BIDDER_KIDOZ) }
pub fn bidder_kiviads() -> BidderName { BidderName::new(BIDDER_KIVIADS) }
pub fn bidder_lm_kiviads() -> BidderName { BidderName::new(BIDDER_LM_KIVIADS) }
pub fn bidder_kobler() -> BidderName { BidderName::new(BIDDER_KOBLER) }
pub fn bidder_krushmedia() -> BidderName { BidderName::new(BIDDER_KRUSHMEDIA) }
pub fn bidder_kueezrtb() -> BidderName { BidderName::new(BIDDER_KUEEZRTB) }
pub fn bidder_lemmadigital() -> BidderName { BidderName::new(BIDDER_LEMMADIGITAL) }
pub fn bidder_limelightdigital() -> BidderName { BidderName::new(BIDDER_LIMELIGHTDIGITAL) }
pub fn bidder_lockerdome() -> BidderName { BidderName::new(BIDDER_LOCKERDOME) }
pub fn bidder_logan() -> BidderName { BidderName::new(BIDDER_LOGAN) }
pub fn bidder_logicad() -> BidderName { BidderName::new(BIDDER_LOGICAD) }
pub fn bidder_loopme() -> BidderName { BidderName::new(BIDDER_LOOPME) }
pub fn bidder_loyal() -> BidderName { BidderName::new(BIDDER_LOYAL) }
pub fn bidder_lunamedia() -> BidderName { BidderName::new(BIDDER_LUNAMEDIA) }
pub fn bidder_mabidder() -> BidderName { BidderName::new(BIDDER_MABIDDER) }
pub fn bidder_madsense() -> BidderName { BidderName::new(BIDDER_MADSENSE) }
pub fn bidder_madvertise() -> BidderName { BidderName::new(BIDDER_MADVERTISE) }
pub fn bidder_marsmedia() -> BidderName { BidderName::new(BIDDER_MARSMEDIA) }
pub fn bidder_mediafuse() -> BidderName { BidderName::new(BIDDER_MEDIAFUSE) }
pub fn bidder_mediago() -> BidderName { BidderName::new(BIDDER_MEDIAGO) }
pub fn bidder_medianet() -> BidderName { BidderName::new(BIDDER_MEDIANET) }
pub fn bidder_mediasquare() -> BidderName { BidderName::new(BIDDER_MEDIASQUARE) }
pub fn bidder_melozen() -> BidderName { BidderName::new(BIDDER_MELOZEN) }
pub fn bidder_metax() -> BidderName { BidderName::new(BIDDER_METAX) }
pub fn bidder_mgid() -> BidderName { BidderName::new(BIDDER_MGID) }
pub fn bidder_mgidx() -> BidderName { BidderName::new(BIDDER_MGIDX) }
pub fn bidder_microsoft() -> BidderName { BidderName::new(BIDDER_MICROSOFT) }
pub fn bidder_minutemedia() -> BidderName { BidderName::new(BIDDER_MINUTEMEDIA) }
pub fn bidder_missena() -> BidderName { BidderName::new(BIDDER_MISSENA) }
pub fn bidder_mobfoxpb() -> BidderName { BidderName::new(BIDDER_MOBFOXPB) }
pub fn bidder_mobilefuse() -> BidderName { BidderName::new(BIDDER_MOBILEFUSE) }
pub fn bidder_mobkoi() -> BidderName { BidderName::new(BIDDER_MOBKOI) }
pub fn bidder_motorik() -> BidderName { BidderName::new(BIDDER_MOTORIK) }
pub fn bidder_nativery() -> BidderName { BidderName::new(BIDDER_NATIVERY) }
pub fn bidder_nativo() -> BidderName { BidderName::new(BIDDER_NATIVO) }
pub fn bidder_nextmillennium() -> BidderName { BidderName::new(BIDDER_NEXTMILLENNIUM) }
pub fn bidder_nexx360() -> BidderName { BidderName::new(BIDDER_NEXX360) }
pub fn bidder_nobid() -> BidderName { BidderName::new(BIDDER_NOBID) }
pub fn bidder_ogury() -> BidderName { BidderName::new(BIDDER_OGURY) }
pub fn bidder_oms() -> BidderName { BidderName::new(BIDDER_OMS) }
pub fn bidder_onetag() -> BidderName { BidderName::new(BIDDER_ONETAG) }
pub fn bidder_openweb() -> BidderName { BidderName::new(BIDDER_OPENWEB) }
pub fn bidder_openx() -> BidderName { BidderName::new(BIDDER_OPENX) }
pub fn bidder_operaads() -> BidderName { BidderName::new(BIDDER_OPERAADS) }
pub fn bidder_optidigital() -> BidderName { BidderName::new(BIDDER_OPTIDIGITAL) }
pub fn bidder_oraki() -> BidderName { BidderName::new(BIDDER_ORAKI) }
pub fn bidder_orbidder() -> BidderName { BidderName::new(BIDDER_ORBIDDER) }
pub fn bidder_outbrain() -> BidderName { BidderName::new(BIDDER_OUTBRAIN) }
pub fn bidder_ownadx() -> BidderName { BidderName::new(BIDDER_OWNADX) }
pub fn bidder_pangle() -> BidderName { BidderName::new(BIDDER_PANGLE) }
pub fn bidder_pgamssp() -> BidderName { BidderName::new(BIDDER_PGAMSSP) }
pub fn bidder_playdigo() -> BidderName { BidderName::new(BIDDER_PLAYDIGO) }
pub fn bidder_pubmatic() -> BidderName { BidderName::new(BIDDER_PUBMATIC) }
pub fn bidder_pubrise() -> BidderName { BidderName::new(BIDDER_PUBRISE) }
pub fn bidder_pubnative() -> BidderName { BidderName::new(BIDDER_PUBNATIVE) }
pub fn bidder_pulsepoint() -> BidderName { BidderName::new(BIDDER_PULSEPOINT) }
pub fn bidder_pwbid() -> BidderName { BidderName::new(BIDDER_PWBID) }
pub fn bidder_qt() -> BidderName { BidderName::new(BIDDER_QT) }
pub fn bidder_readpeak() -> BidderName { BidderName::new(BIDDER_READPEAK) }
pub fn bidder_rediads() -> BidderName { BidderName::new(BIDDER_REDIADS) }
pub fn bidder_relevantdigital() -> BidderName { BidderName::new(BIDDER_RELEVANTDIGITAL) }
pub fn bidder_resetdigital() -> BidderName { BidderName::new(BIDDER_RESETDIGITAL) }
pub fn bidder_revcontent() -> BidderName { BidderName::new(BIDDER_REVCONTENT) }
pub fn bidder_richaudience() -> BidderName { BidderName::new(BIDDER_RICHAUDIENCE) }
pub fn bidder_rise() -> BidderName { BidderName::new(BIDDER_RISE) }
pub fn bidder_roulax() -> BidderName { BidderName::new(BIDDER_ROULAX) }
pub fn bidder_rtbhouse() -> BidderName { BidderName::new(BIDDER_RTBHOUSE) }
pub fn bidder_rubicon() -> BidderName { BidderName::new(BIDDER_RUBICON) }
pub fn bidder_seedingalliance() -> BidderName { BidderName::new(BIDDER_SEEDINGALLIANCE) }
pub fn bidder_seedtag() -> BidderName { BidderName::new(BIDDER_SEEDTAG) }
pub fn bidder_sa_lunamedia() -> BidderName { BidderName::new(BIDDER_SA_LUNAMEDIA) }
pub fn bidder_sharethrough() -> BidderName { BidderName::new(BIDDER_SHARETHROUGH) }
pub fn bidder_showheroes() -> BidderName { BidderName::new(BIDDER_SHOWHEROES) }
pub fn bidder_silvermob() -> BidderName { BidderName::new(BIDDER_SILVERMOB) }
pub fn bidder_silverpush() -> BidderName { BidderName::new(BIDDER_SILVERPUSH) }
pub fn bidder_smaato() -> BidderName { BidderName::new(BIDDER_SMAATO) }
pub fn bidder_smartadserver() -> BidderName { BidderName::new(BIDDER_SMARTADSERVER) }
pub fn bidder_smarthub() -> BidderName { BidderName::new(BIDDER_SMARTHUB) }
pub fn bidder_smartrtb() -> BidderName { BidderName::new(BIDDER_SMARTRTB) }
pub fn bidder_smartx() -> BidderName { BidderName::new(BIDDER_SMARTX) }
pub fn bidder_smartyads() -> BidderName { BidderName::new(BIDDER_SMARTYADS) }
pub fn bidder_smilewanted() -> BidderName { BidderName::new(BIDDER_SMILEWANTED) }
pub fn bidder_smoot() -> BidderName { BidderName::new(BIDDER_SMOOT) }
pub fn bidder_smrtconnect() -> BidderName { BidderName::new(BIDDER_SMRTCONNECT) }
pub fn bidder_sonobi() -> BidderName { BidderName::new(BIDDER_SONOBI) }
pub fn bidder_sovrn() -> BidderName { BidderName::new(BIDDER_SOVRN) }
pub fn bidder_sovrnxsp() -> BidderName { BidderName::new(BIDDER_SOVRNXSP) }
pub fn bidder_sparteo() -> BidderName { BidderName::new(BIDDER_SPARTEO) }
pub fn bidder_sspbc() -> BidderName { BidderName::new(BIDDER_SSPBC) }
pub fn bidder_startio() -> BidderName { BidderName::new(BIDDER_STARTIO) }
pub fn bidder_stroeercore() -> BidderName { BidderName::new(BIDDER_STROEERCORE) }
pub fn bidder_taboola() -> BidderName { BidderName::new(BIDDER_TABOOLA) }
pub fn bidder_tappx() -> BidderName { BidderName::new(BIDDER_TAPPX) }
pub fn bidder_teads() -> BidderName { BidderName::new(BIDDER_TEADS) }
pub fn bidder_telaria() -> BidderName { BidderName::new(BIDDER_TELARIA) }
pub fn bidder_teqblaze() -> BidderName { BidderName::new(BIDDER_TEQBLAZE) }
pub fn bidder_theadx() -> BidderName { BidderName::new(BIDDER_THEADX) }
pub fn bidder_thetradedesk() -> BidderName { BidderName::new(BIDDER_THETRADEDESK) }
pub fn bidder_tpmn() -> BidderName { BidderName::new(BIDDER_TPMN) }
pub fn bidder_tradplus() -> BidderName { BidderName::new(BIDDER_TRADPLUS) }
pub fn bidder_trafficgate() -> BidderName { BidderName::new(BIDDER_TRAFFICGATE) }
pub fn bidder_triplelift() -> BidderName { BidderName::new(BIDDER_TRIPLELIFT) }
pub fn bidder_triplelift_native() -> BidderName { BidderName::new(BIDDER_TRIPLELIFT_NATIVE) }
pub fn bidder_trustedstack() -> BidderName { BidderName::new(BIDDER_TRUSTEDSTACK) }
pub fn bidder_trustx() -> BidderName { BidderName::new(BIDDER_TRUSTX) }
pub fn bidder_ucfunnel() -> BidderName { BidderName::new(BIDDER_UCFUNNEL) }
pub fn bidder_undertone() -> BidderName { BidderName::new(BIDDER_UNDERTONE) }
pub fn bidder_unicorn() -> BidderName { BidderName::new(BIDDER_UNICORN) }
pub fn bidder_unruly() -> BidderName { BidderName::new(BIDDER_UNRULY) }
pub fn bidder_vidazoo() -> BidderName { BidderName::new(BIDDER_VIDAZOO) }
pub fn bidder_videobyte() -> BidderName { BidderName::new(BIDDER_VIDEOBYTE) }
pub fn bidder_videoheroes() -> BidderName { BidderName::new(BIDDER_VIDEOHEROES) }
pub fn bidder_vidoomy() -> BidderName { BidderName::new(BIDDER_VIDOOMY) }
pub fn bidder_visiblemeasures() -> BidderName { BidderName::new(BIDDER_VISIBLEMEASURES) }
pub fn bidder_visx() -> BidderName { BidderName::new(BIDDER_VISX) }
pub fn bidder_vox() -> BidderName { BidderName::new(BIDDER_VOX) }
pub fn bidder_vrtcal() -> BidderName { BidderName::new(BIDDER_VRTCAL) }
pub fn bidder_vungle() -> BidderName { BidderName::new(BIDDER_VUNGLE) }
pub fn bidder_xeworks() -> BidderName { BidderName::new(BIDDER_XEWORKS) }
pub fn bidder_yahooads() -> BidderName { BidderName::new(BIDDER_YAHOOADS) }
pub fn bidder_yandex() -> BidderName { BidderName::new(BIDDER_YANDEX) }
pub fn bidder_yeahmobi() -> BidderName { BidderName::new(BIDDER_YEAHMOBI) }
pub fn bidder_yieldlab() -> BidderName { BidderName::new(BIDDER_YIELDLAB) }
pub fn bidder_yieldmo() -> BidderName { BidderName::new(BIDDER_YIELDMO) }
pub fn bidder_yieldone() -> BidderName { BidderName::new(BIDDER_YIELDONE) }
pub fn bidder_zentotem() -> BidderName { BidderName::new(BIDDER_ZENTOTEM) }
pub fn bidder_zeroclickfraud() -> BidderName { BidderName::new(BIDDER_ZEROCLICKFRAUD) }
pub fn bidder_zetaglobalssp() -> BidderName { BidderName::new(BIDDER_ZETAGLOBALSSP) }
pub fn bidder_zmaticoo() -> BidderName { BidderName::new(BIDDER_ZMATICOO) }

/// Returns a Vec of all core bidder names.
pub fn all_bidder_names() -> Vec<BidderName> {
    vec![
        bidder_33across(),
        bidder_aax(),
        bidder_aceex(),
        bidder_acuityads(),
        bidder_adagio(),
        bidder_adelement(),
        bidder_adf(),
        bidder_adgeneration(),
        bidder_adhese(),
        bidder_adkernel(),
        bidder_adkerneladn(),
        bidder_adman(),
        bidder_admatic(),
        bidder_admixer(),
        bidder_adnuntius(),
        bidder_adot(),
        bidder_adpone(),
        bidder_adprime(),
        bidder_adquery(),
        bidder_adrino(),
        bidder_adtarget(),
        bidder_adtrgtme(),
        bidder_adtonos(),
        bidder_adtelligent(),
        bidder_aduptech(),
        bidder_advangelists(),
        bidder_adverxo(),
        bidder_adview(),
        bidder_adxcg(),
        bidder_adyoulike(),
        bidder_afront(),
        bidder_aidem(),
        bidder_aja(),
        bidder_akcelo(),
        bidder_algorix(),
        bidder_alkimi(),
        bidder_alliance_gravity(),
        bidder_amx(),
        bidder_apacdex(),
        bidder_appnexus(),
        bidder_appush(),
        bidder_aso(),
        bidder_audiencenetwork(),
        bidder_automatad(),
        bidder_avocet(),
        bidder_axis(),
        bidder_axonix(),
        bidder_beachfront(),
        bidder_beintoo(),
        bidder_bematterfull(),
        bidder_beop(),
        bidder_between(),
        bidder_beyondmedia(),
        bidder_bidmachine(),
        bidder_bidmatic(),
        bidder_bidmyadz(),
        bidder_bidscube(),
        bidder_bidstack(),
        bidder_bidtheatre(),
        bidder_bigoad(),
        bidder_blasto(),
        bidder_bliink(),
        bidder_blis(),
        bidder_blue(),
        bidder_bluesea(),
        bidder_bmtm(),
        bidder_boldwin(),
        bidder_boldwin_rapid(),
        bidder_brave(),
        bidder_bwx(),
        bidder_cadent_aperture_mx(),
        bidder_ccx(),
        bidder_clydo(),
        bidder_cointraffic(),
        bidder_coinzilla(),
        bidder_colossus(),
        bidder_compass(),
        bidder_concert(),
        bidder_connatix(),
        bidder_connectad(),
        bidder_consumable(),
        bidder_contxtful(),
        bidder_conversant(),
        bidder_copper6ssp(),
        bidder_cpmstar(),
        bidder_criteo(),
        bidder_cwire(),
        bidder_datablocks(),
        bidder_decenterads(),
        bidder_deepintent(),
        bidder_definemedia(),
        bidder_dianomi(),
        bidder_displayio(),
        bidder_edge226(),
        bidder_dmx(),
        bidder_driftpixel(),
        bidder_elementaltv(),
        bidder_emtv(),
        bidder_emx_digital(),
        bidder_eplanning(),
        bidder_epom(),
        bidder_escalax(),
        bidder_exco(),
        bidder_e_volution(),
        bidder_feedad(),
        bidder_flatads(),
        bidder_flipp(),
        bidder_freewheelssp(),
        bidder_fwssp(),
        bidder_frvradn(),
        bidder_gamma(),
        bidder_gamoshi(),
        bidder_globalsun(),
        bidder_goldbach(),
        bidder_grid(),
        bidder_gumgum(),
        bidder_huaweiads(),
        bidder_imds(),
        bidder_impactify(),
        bidder_improvedigital(),
        bidder_infytv(),
        bidder_inmobi(),
        bidder_insticator(),
        bidder_intenze(),
        bidder_interactiveoffers(),
        bidder_invibes(),
        bidder_iqx(),
        bidder_iqzone(),
        bidder_ix(),
        bidder_jixie(),
        bidder_kargo(),
        bidder_kayzen(),
        bidder_kidoz(),
        bidder_kiviads(),
        bidder_lm_kiviads(),
        bidder_kobler(),
        bidder_krushmedia(),
        bidder_kueezrtb(),
        bidder_lemmadigital(),
        bidder_limelightdigital(),
        bidder_lockerdome(),
        bidder_logan(),
        bidder_logicad(),
        bidder_loopme(),
        bidder_loyal(),
        bidder_lunamedia(),
        bidder_mabidder(),
        bidder_madsense(),
        bidder_madvertise(),
        bidder_marsmedia(),
        bidder_mediafuse(),
        bidder_mediago(),
        bidder_medianet(),
        bidder_mediasquare(),
        bidder_melozen(),
        bidder_metax(),
        bidder_mgid(),
        bidder_mgidx(),
        bidder_microsoft(),
        bidder_minutemedia(),
        bidder_missena(),
        bidder_mobfoxpb(),
        bidder_mobilefuse(),
        bidder_mobkoi(),
        bidder_motorik(),
        bidder_nativery(),
        bidder_nativo(),
        bidder_nextmillennium(),
        bidder_nexx360(),
        bidder_nobid(),
        bidder_ogury(),
        bidder_oms(),
        bidder_onetag(),
        bidder_openweb(),
        bidder_openx(),
        bidder_operaads(),
        bidder_optidigital(),
        bidder_oraki(),
        bidder_orbidder(),
        bidder_outbrain(),
        bidder_ownadx(),
        bidder_pangle(),
        bidder_pgamssp(),
        bidder_playdigo(),
        bidder_pubmatic(),
        bidder_pubrise(),
        bidder_pubnative(),
        bidder_pulsepoint(),
        bidder_pwbid(),
        bidder_qt(),
        bidder_readpeak(),
        bidder_rediads(),
        bidder_relevantdigital(),
        bidder_resetdigital(),
        bidder_revcontent(),
        bidder_richaudience(),
        bidder_rise(),
        bidder_roulax(),
        bidder_rtbhouse(),
        bidder_rubicon(),
        bidder_seedingalliance(),
        bidder_seedtag(),
        bidder_sa_lunamedia(),
        bidder_sharethrough(),
        bidder_showheroes(),
        bidder_silvermob(),
        bidder_silverpush(),
        bidder_smaato(),
        bidder_smartadserver(),
        bidder_smarthub(),
        bidder_smartrtb(),
        bidder_smartx(),
        bidder_smartyads(),
        bidder_smilewanted(),
        bidder_smoot(),
        bidder_smrtconnect(),
        bidder_sonobi(),
        bidder_sovrn(),
        bidder_sovrnxsp(),
        bidder_sparteo(),
        bidder_sspbc(),
        bidder_startio(),
        bidder_stroeercore(),
        bidder_taboola(),
        bidder_tappx(),
        bidder_teads(),
        bidder_telaria(),
        bidder_teqblaze(),
        bidder_theadx(),
        bidder_thetradedesk(),
        bidder_tpmn(),
        bidder_tradplus(),
        bidder_trafficgate(),
        bidder_triplelift(),
        bidder_triplelift_native(),
        bidder_trustedstack(),
        bidder_trustx(),
        bidder_ucfunnel(),
        bidder_undertone(),
        bidder_unicorn(),
        bidder_unruly(),
        bidder_vidazoo(),
        bidder_videobyte(),
        bidder_videoheroes(),
        bidder_vidoomy(),
        bidder_visiblemeasures(),
        bidder_visx(),
        bidder_vox(),
        bidder_vrtcal(),
        bidder_vungle(),
        bidder_xeworks(),
        bidder_yahooads(),
        bidder_yandex(),
        bidder_yeahmobi(),
        bidder_yieldlab(),
        bidder_yieldmo(),
        bidder_yieldone(),
        bidder_zentotem(),
        bidder_zeroclickfraud(),
        bidder_zetaglobalssp(),
        bidder_zmaticoo(),
    ]
}
