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
pub const BIDDER_ZETAGLOBALSSP: &str = "zetaglobalssp";
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
