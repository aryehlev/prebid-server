//! Bidder-specific impression extension types.
//! Auto-generated from Go openrtb_ext/imp_*.go files.

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtAceex {
    #[serde(rename = "accountid")]
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtAcuityAds {
    pub host: String,
    #[serde(rename = "accountid")]
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtAdelement {
    pub supply_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtAdpone {
    #[serde(rename = "placementId", skip_serializing_if = "Option::is_none")]
    pub placement_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtAfront {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "sourceId")]
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtBematterfull {
    pub env: String,
    pub pid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtBlasto {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "sourceId")]
    pub source_id: String,
    pub host: String,
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtBWX {
    pub env: String,
    pub pid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtDriftPixel {
    pub env: String,
    pub pid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtEscalax {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "sourceId")]
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImp33across {
    #[serde(rename = "zoneId", skip_serializing_if = "Option::is_none")]
    pub zone_id: Option<String>,
    #[serde(rename = "siteId", skip_serializing_if = "Option::is_none")]
    pub site_id: Option<String>,
    #[serde(rename = "productId", skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAax {
    pub cid: String,
    pub crid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdagio {
    #[serde(rename = "organizationId")]
    pub organization_id: String,
    pub placement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagetype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdf {
    #[serde(rename = "mid", skip_serializing_if = "Option::is_none")]
    pub master_tag_id: Option<serde_json::Number>,
    #[serde(rename = "inv", skip_serializing_if = "Option::is_none")]
    pub inventory_source_id: Option<i64>,
    #[serde(rename = "mname", skip_serializing_if = "Option::is_none")]
    pub placement_name: Option<String>,
    #[serde(rename = "priceType", skip_serializing_if = "Option::is_none")]
    pub price_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdgeneration {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdhese {
    pub account: String,
    pub location: String,
    pub format: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub targets: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdkernel {
    #[serde(rename = "zoneId")]
    pub zone_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdkernelAdn {
    #[serde(rename = "pubId")]
    pub publisher_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdman {
    #[serde(rename = "TagID")]
    pub tag_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdmixer {
    #[serde(rename = "zone")]
    pub zone_id: String,
    #[serde(rename = "customFloor")]
    pub custom_bid_floor: f64,
    #[serde(rename = "customParams")]
    pub custom_params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallax: Option<bool>,
    #[serde(rename = "placementId", skip_serializing_if = "Option::is_none")]
    pub placement_id: Option<String>,
    #[serde(rename = "publisherPath", skip_serializing_if = "Option::is_none")]
    pub publisher_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdprime {
    #[serde(rename = "TagID")]
    pub tag_id: String,
    pub keywords: Vec<String>,
    pub audiences: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdrino {
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdtarget {
    #[serde(rename = "aid")]
    pub source_id: serde_json::Number,
    #[serde(rename = "placementId", skip_serializing_if = "Option::is_none")]
    pub placement_id: Option<i64>,
    #[serde(rename = "siteId", skip_serializing_if = "Option::is_none")]
    pub site_id: Option<i64>,
    #[serde(rename = "bidFloor", skip_serializing_if = "Option::is_none")]
    pub bid_floor: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdtelligent {
    #[serde(rename = "aid")]
    pub source_id: serde_json::Number,
    #[serde(rename = "placementId", skip_serializing_if = "Option::is_none")]
    pub placement_id: Option<i64>,
    #[serde(rename = "siteId", skip_serializing_if = "Option::is_none")]
    pub site_id: Option<i64>,
    #[serde(rename = "bidFloor", skip_serializing_if = "Option::is_none")]
    pub bid_floor: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdtrgtme {
    pub site_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrebidInline {
    #[serde(rename = "adunitcode", skip_serializing_if = "Option::is_none")]
    pub ad_unit_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdUnitCode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prebid: Option<PrebidInline>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAduptech {
    pub publisher: String,
    pub placement: String,
    pub query: String,
    #[serde(rename = "adtest")]
    pub ad_test: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdvangelists {
    #[serde(rename = "pubid")]
    pub publisher_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdView {
    #[serde(rename = "placementId")]
    pub master_tag_id: String,
    #[serde(rename = "accountId")]
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAdyoulike {
    #[serde(rename = "placement")]
    pub placement_id: String,
    pub campaign: String,
    pub track: String,
    pub creative: String,
    pub source: String,
    pub debug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAidem {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "siteId")]
    pub site_id: String,
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
    #[serde(rename = "rateLimit")]
    pub rate_limit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAJA {
    #[serde(rename = "asi")]
    pub ad_spot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAkcelo {
    #[serde(rename = "adUnitId", skip_serializing_if = "Option::is_none")]
    pub ad_unit_id: Option<serde_json::Number>,
    #[serde(rename = "siteId", skip_serializing_if = "Option::is_none")]
    pub site_id: Option<serde_json::Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<serde_json::Number>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAlgorix {
    pub sid: String,
    pub token: String,
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAlkimi {
    pub token: String,
    #[serde(rename = "bidFloor")]
    pub bid_floor: f64,
    pub instl: i8,
    pub exp: i64,
    #[serde(rename = "adUnitCode")]
    pub ad_unit_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAllianceGravity {
    #[serde(rename = "srid")]
    pub sr_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAMX {
    #[serde(rename = "tagId", skip_serializing_if = "Option::is_none")]
    pub tag_id: Option<String>,
    #[serde(rename = "adUnitId", skip_serializing_if = "Option::is_none")]
    pub ad_unit_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpApacdex {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "siteId")]
    pub site_id: String,
    #[serde(rename = "floorPrice")]
    pub floor_price: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAppnexus {
    #[serde(rename = "placementId")]
    pub deprecated_placement_id: serde_json::Value,
    #[serde(rename = "invCode")]
    pub legacy_inv_code: String,
    #[serde(rename = "trafficSourceCode")]
    pub legacy_traffic_source_code: String,
    pub placement_id: serde_json::Value,
    pub inv_code: String,
    pub member: serde_json::Value,
    pub keywords: ExtImpAppnexusKeywords,
    pub traffic_source_code: String,
    pub reserve: f64,
    pub position: String,
    #[serde(rename = "use_pmt_rule", skip_serializing_if = "Option::is_none")]
    pub use_payment_rule: Option<bool>,
    #[serde(rename = "use_payment_rule", skip_serializing_if = "Option::is_none")]
    pub deprecated_use_payment_rule: Option<bool>,
    pub private_sizes: serde_json::Value,
    #[serde(rename = "generate_ad_pod_id")]
    pub ad_pod_id: bool,
    pub ext_inv_code: String,
    pub external_imp_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAppnexusKeyVal {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(rename = "value", skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

pub type ExtImpAppnexusKeywords = String;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAso {
    pub zone: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAvocet {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpAxonix {
    #[serde(rename = "supplyId")]
    pub supply_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpBeachfront {
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "appIds")]
    pub app_ids: ExtImpBeachfrontAppIds,
    #[serde(rename = "bidfloor")]
    pub bid_floor: f64,
    #[serde(rename = "videoResponseType", skip_serializing_if = "Option::is_none")]
    pub video_response_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpBeachfrontAppIds {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpBeintoo {
    #[serde(rename = "tagid")]
    pub tag_id: String,
    #[serde(rename = "bidfloor", skip_serializing_if = "Option::is_none")]
    pub bid_floor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpBeop {
    #[serde(rename = "pid", skip_serializing_if = "Option::is_none")]
    pub beop_publisher_id: Option<String>,
    #[serde(rename = "nid", skip_serializing_if = "Option::is_none")]
    pub beop_network_id: Option<String>,
    #[serde(rename = "nptnid", skip_serializing_if = "Option::is_none")]
    pub beop_network_partner_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpBetween {
    pub host: String,
    pub publisher_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpBidmachine {
    pub host: String,
    pub path: String,
    pub seller_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpBidmatic {
    #[serde(rename = "source")]
    pub source_id: serde_json::Number,
    #[serde(rename = "placementId", skip_serializing_if = "Option::is_none")]
    pub placement_id: Option<i64>,
    #[serde(rename = "siteId", skip_serializing_if = "Option::is_none")]
    pub site_id: Option<i64>,
    #[serde(rename = "bidFloor", skip_serializing_if = "Option::is_none")]
    pub bid_floor: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpBidsCube {
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpBigoAd {
    #[serde(rename = "sspid")]
    pub ssp_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpBluesea {
    #[serde(rename = "pubid")]
    pub pub_id: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpBrave {
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpCadentApertureMX {
    #[serde(rename = "tagid")]
    pub tag_id: String,
    #[serde(rename = "bidfloor", skip_serializing_if = "Option::is_none")]
    pub bid_floor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpCcx {
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpCointraffic {
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpColossus {
    #[serde(rename = "TagID")]
    pub tag_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpConnatix {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "viewabilityPercentage")]
    pub viewability_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpConnectAd {
    #[serde(rename = "networkId")]
    pub network_id: serde_json::Value,
    #[serde(rename = "siteId")]
    pub site_id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidfloor: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpConsumable {
    #[serde(rename = "networkId", skip_serializing_if = "Option::is_none")]
    pub network_id: Option<i64>,
    #[serde(rename = "siteId", skip_serializing_if = "Option::is_none")]
    pub site_id: Option<i64>,
    #[serde(rename = "unitId", skip_serializing_if = "Option::is_none")]
    pub unit_id: Option<i64>,
    #[serde(rename = "unitName", skip_serializing_if = "Option::is_none")]
    pub unit_name: Option<String>,
    #[serde(rename = "placementid", skip_serializing_if = "Option::is_none")]
    pub placement_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpContxtful {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "customerId")]
    pub customer_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpConversant {
    pub site_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure: Option<i8>,
    pub tag_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i8>,
    #[serde(rename = "bidfloor")]
    pub bid_floor: f64,
    #[serde(rename = "mimes")]
    pub mim_es: Vec<String>,
    pub api: Vec<i8>,
    pub protocols: Vec<i8>,
    #[serde(rename = "maxduration", skip_serializing_if = "Option::is_none")]
    pub max_duration: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpCpmstar {
    #[serde(rename = "placementId")]
    pub pool_id: i64,
    #[serde(rename = "subpoolId", skip_serializing_if = "Option::is_none")]
    pub sub_pool_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpCriteo {
    #[serde(rename = "zoneId")]
    pub zone_id: i64,
    #[serde(rename = "networkId")]
    pub network_id: i64,
    pub uid: i64,
    #[serde(rename = "pubid")]
    pub pub_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpDatablocks {
    #[serde(rename = "sourceId")]
    pub source_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpDecenterAds {
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpDeepintent {
    #[serde(rename = "tagId")]
    pub tag_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpDianomi {
    #[serde(rename = "smartadId", skip_serializing_if = "Option::is_none")]
    pub smartad_id: Option<serde_json::Number>,
    #[serde(rename = "priceType", skip_serializing_if = "Option::is_none")]
    pub price_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpDisplayio {
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
    #[serde(rename = "inventoryId")]
    pub inventory_id: String,
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpElementalTV {
    #[serde(rename = "adunit")]
    pub ad_unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpEPlanning {
    #[serde(rename = "ci")]
    pub client_id: String,
    #[serde(rename = "adunit_code")]
    pub ad_unit_code: String,
    pub size_string: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpFacebook {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpFeedAd {
    #[serde(rename = "clientToken")]
    pub client_token: String,
    pub decoration: String,
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "sdkOptions", skip_serializing_if = "Option::is_none")]
    pub sdk_options: Option<ExtImpFeedAdSdkOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpFeedAdSdkOptions {
    pub advertising_id: String,
    pub app_name: String,
    pub bundle_id: String,
    pub hybrid_app: bool,
    pub hybrid_platform: String,
    pub limit_ad_tracking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpGamma {
    #[serde(rename = "id")]
    pub partner_id: String,
    #[serde(rename = "zid")]
    pub zone_id: String,
    #[serde(rename = "wid")]
    pub web_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpGamoshi {
    #[serde(rename = "supplyPartnerId")]
    pub supply_partner_id: String,
    #[serde(rename = "favoredMediaType")]
    pub favored_media_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpGrid {
    pub uid: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpGumGum {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
    #[serde(rename = "pubId", skip_serializing_if = "Option::is_none")]
    pub pub_id: Option<f64>,
    #[serde(rename = "irisid", skip_serializing_if = "Option::is_none")]
    pub iris_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpGumGumBanner {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub si: Option<f64>,
    #[serde(rename = "maxw", skip_serializing_if = "Option::is_none")]
    pub max_w: Option<f64>,
    #[serde(rename = "maxh", skip_serializing_if = "Option::is_none")]
    pub max_h: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpGumGumVideo {
    #[serde(rename = "irisid", skip_serializing_if = "Option::is_none")]
    pub iris_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpHuaweiAds {
    #[serde(rename = "slotid")]
    pub slot_id: String,
    pub adtype: String,
    #[serde(rename = "publisherid")]
    pub publisher_id: String,
    #[serde(rename = "signkey")]
    pub sign_key: String,
    #[serde(rename = "keyid")]
    pub key_id: String,
    #[serde(rename = "isTestAuthorization", skip_serializing_if = "Option::is_none")]
    pub is_test_authorization: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpImds {
    #[serde(rename = "seatId")]
    pub seat_id: String,
    #[serde(rename = "tagId")]
    pub tag_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpImpactify {
    #[serde(rename = "appId")]
    pub app_id: String,
    pub format: String,
    pub style: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpInMobi {
    pub plc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpInsticator {
    #[serde(rename = "adUnitId", skip_serializing_if = "Option::is_none")]
    pub ad_unit_id: Option<String>,
    #[serde(rename = "publisherId", skip_serializing_if = "Option::is_none")]
    pub publisher_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpInteractiveoffers {
    #[serde(rename = "partnerId")]
    pub partner_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpInvibes {
    #[serde(rename = "placementId", skip_serializing_if = "Option::is_none")]
    pub placement_id: Option<String>,
    #[serde(rename = "domainId")]
    pub domain_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<ExtImpInvibesDebug>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpInvibesDebug {
    #[serde(rename = "testBvid", skip_serializing_if = "Option::is_none")]
    pub test_bvid: Option<String>,
    #[serde(rename = "testLog", skip_serializing_if = "Option::is_none")]
    pub test_log: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpIx {
    #[serde(rename = "siteId")]
    pub site_id: String,
    pub size: Vec<i64>,
    pub sid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpJixie {
    pub unit: String,
    #[serde(rename = "accountid", skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(rename = "jxprop1", skip_serializing_if = "Option::is_none")]
    pub jx_prop1: Option<String>,
    #[serde(rename = "jxprop2", skip_serializing_if = "Option::is_none")]
    pub jx_prop2: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpKidoz {
    pub access_token: String,
    pub publisher_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpKobler {
    pub test: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpLockerDome {
    #[serde(rename = "adUnitId")]
    pub ad_unit_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpLogicad {
    pub tid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpLoopme {
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
    #[serde(rename = "bundleId")]
    pub bundle_id: String,
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpLunaMedia {
    #[serde(rename = "pubid")]
    pub publisher_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpMadSense {
    pub company_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpMadvertise {
    #[serde(rename = "zoneId")]
    pub zone_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpMarsmedia {
    #[serde(rename = "zoneId")]
    pub zone_id: serde_json::Number,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpMediaGo {
    pub token: String,
    pub region: String,
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpMedianet {
    pub cid: String,
    pub crid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpMetaX {
    #[serde(rename = "publisherId")]
    pub publisher_id: i64,
    pub adunit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpMgid {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "placementId", skip_serializing_if = "Option::is_none")]
    pub placement_id: Option<String>,
    pub cur: String,
    pub currency: String,
    #[serde(rename = "bidfloor")]
    pub bid_floor: f64,
    #[serde(rename = "bidFloor")]
    pub bid_floor2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpMissena {
    #[serde(rename = "apiKey")]
    pub api_key: String,
    pub formats: Vec<String>,
    pub placement: String,
    pub sample: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub settings: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpMobileFuse {
    pub placement_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpNexx360 {
    #[serde(rename = "tagId", skip_serializing_if = "Option::is_none")]
    pub tag_id: Option<String>,
    pub placement: string `json:"placement,omitempty"` // Placement ID,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpNoBid {
    #[serde(rename = "siteId")]
    pub site_id: String,
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpOms {
    pub pid: String,
    #[serde(rename = "publisherId")]
    pub publisher_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpOnetag {
    #[serde(rename = "pubId")]
    pub pub_id: String,
    pub ext: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpOpenWeb {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aid: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpOpenx {
    pub unit: serde_json::Number,
    pub platform: String,
    #[serde(rename = "delDomain")]
    pub del_domain: String,
    #[serde(rename = "customFloor")]
    pub custom_floor: serde_json::Number,
    #[serde(rename = "customParams")]
    pub custom_params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpOrbidder {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "bidfloor")]
    pub bid_floor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpOutbrain {
    pub publisher: ExtImpOutbrainPublisher,
    #[serde(rename = "tagid")]
    pub tag_id: String,
    #[serde(rename = "bcat")]
    pub b_cat: Vec<String>,
    #[serde(rename = "badv")]
    pub b_adv: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpOutbrainPublisher {
    pub id: String,
    pub name: String,
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpOwnAdx {
    #[serde(rename = "sspId")]
    pub ssp_id: String,
    #[serde(rename = "seatId")]
    pub seat_id: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpPubmatic {
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
    #[serde(rename = "adSlot")]
    pub ad_slot: String,
    pub dctr: String,
    #[serde(rename = "pmzoneid")]
    pub pm_zone_id: String,
    #[serde(rename = "wrapper", skip_serializing_if = "Option::is_none")]
    pub wrap_ext: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<Option<ExtImpPubmaticKeyVal>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kadfloor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpPubmaticKeyVal {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(rename = "value", skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpPubnative {
    pub zone_id: i64,
    pub app_auth_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpPulsePoint {
    #[serde(rename = "cp")]
    pub pub_id: serde_json::Value,
    #[serde(rename = "ct")]
    pub tag_id: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpPwbid {
    #[serde(rename = "siteId")]
    pub site_id: String,
    #[serde(rename = "bidFloor")]
    pub bid_floor: f32,
    #[serde(rename = "isTest")]
    pub is_test: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpRediads {
    pub account_id: String,
    pub slot: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpRichaudience {
    pub pid: String,
    #[serde(rename = "supplyType")]
    pub supply_type: String,
    #[serde(rename = "bidfloor")]
    pub bid_floor: f64,
    #[serde(rename = "bidfloorcur")]
    pub bid_floor_cur: String,
    pub test: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpRoulax {
    #[serde(rename = "Pid", skip_serializing_if = "Option::is_none")]
    pub pid: Option<String>,
    #[serde(rename = "publisherPath", skip_serializing_if = "Option::is_none")]
    pub publisher_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpRTBHouse {
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
    pub region: String,
    #[serde(rename = "bidfloor", skip_serializing_if = "Option::is_none")]
    pub bid_floor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpRubicon {
    #[serde(rename = "accountId")]
    pub account_id: serde_json::Number,
    #[serde(rename = "siteId")]
    pub site_id: serde_json::Number,
    #[serde(rename = "zoneId")]
    pub zone_id: serde_json::Number,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inventory: Option<serde_json::Value>,
    #[serde(rename = "bidonmultiformat", skip_serializing_if = "Option::is_none")]
    pub bid_on_multiformat: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visitor: Option<serde_json::Value>,
    pub video: RubiconVideoParams,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<ImpExtRubiconDebug>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpSaLunamedia {
    pub key: String,
    pub r#type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpSharethrough {
    pub pkey: String,
    #[serde(rename = "badv", skip_serializing_if = "Vec::is_empty")]
    pub b_adv: Vec<String>,
    #[serde(rename = "bcat", skip_serializing_if = "Vec::is_empty")]
    pub b_cat: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpShowheroes {
    #[serde(rename = "unitId")]
    pub unit_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpSmaato {
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
    #[serde(rename = "adspaceId")]
    pub ad_space_id: String,
    #[serde(rename = "adbreakId")]
    pub ad_break_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpSmartadserverIn {
    #[serde(rename = "siteId")]
    pub site_id: i64,
    #[serde(rename = "pageId")]
    pub page_id: i64,
    #[serde(rename = "formatId")]
    pub format_id: i64,
    #[serde(rename = "networkId")]
    pub network_id: i64,
    #[serde(rename = "programmaticGuaranteed")]
    pub programmatic_guaranteed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpSmartadserverOut {
    #[serde(rename = "siteId")]
    pub site_id: i64,
    #[serde(rename = "pageId")]
    pub page_id: i64,
    #[serde(rename = "formatId")]
    pub format_id: i64,
    #[serde(rename = "networkId")]
    pub network_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpSmartclip {
    #[serde(rename = "tagId")]
    pub tag_id: String,
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
    #[serde(rename = "siteId")]
    pub site_id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "bundleId")]
    pub bundle_id: String,
    #[serde(rename = "storeUrl")]
    pub store_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpSmartRTB {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pub_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub med_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_bid: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpSmilewanted {
    #[serde(rename = "zoneId")]
    pub zone_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpSonobi {
    #[serde(rename = "tagid")]
    pub tag_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpSovrn {
    #[serde(rename = "tagId", skip_serializing_if = "Option::is_none")]
    pub tag_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagid: Option<String>,
    #[serde(rename = "bidfloor", skip_serializing_if = "Option::is_none")]
    pub bid_floor: Option<serde_json::Value>,
    #[serde(rename = "adunitcode", skip_serializing_if = "Option::is_none")]
    pub ad_unit_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpSovrnXsp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pub_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub med_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_bid: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpSparteo {
    #[serde(rename = "networkId")]
    pub network_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom4: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom5: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpSspbc {
    #[serde(rename = "siteId")]
    pub site_id: String,
    pub id: String,
    #[serde(rename = "test")]
    pub is_test: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpStroeerCore {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpTappx {
    pub host: string   `json:"host,omitempty"` //DEPRECATED,
    #[serde(rename = "tappxkey")]
    pub tappx_key: String,
    pub endpoint: String,
    #[serde(rename = "bidfloor", skip_serializing_if = "Option::is_none")]
    pub bid_floor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mktag: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bcid: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bcrid: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpTeads {
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpTelaria {
    #[serde(rename = "adCode", skip_serializing_if = "Option::is_none")]
    pub ad_code: Option<String>,
    #[serde(rename = "seatCode")]
    pub seat_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpTheadx {
    #[serde(rename = "tagid")]
    pub tag_id: serde_json::Number,
    #[serde(rename = "wid", skip_serializing_if = "Option::is_none")]
    pub inventory_source_id: Option<i64>,
    #[serde(rename = "pid", skip_serializing_if = "Option::is_none")]
    pub member_id: Option<i64>,
    #[serde(rename = "pname", skip_serializing_if = "Option::is_none")]
    pub placement_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpTheTradeDesk {
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
    #[serde(rename = "supplySourceId")]
    pub supply_source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpTpmn {
    #[serde(rename = "inventoryId")]
    pub inventory_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpTradPlus {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "zoneId")]
    pub zone_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpTrafficGate {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    pub host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpTriplelift {
    #[serde(rename = "inventoryCode")]
    pub inv_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floor: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpTrustedstack {
    pub cid: String,
    pub crid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpTrustX {
    pub uid: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpUcfunnel {
    #[serde(rename = "adunitid")]
    pub ad_unit_id: String,
    #[serde(rename = "partnerid")]
    pub partner_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpUndertone {
    #[serde(rename = "publisherId")]
    pub publisher_id: i64,
    #[serde(rename = "placementId")]
    pub placement_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpUnicorn {
    #[serde(rename = "placementId", skip_serializing_if = "Option::is_none")]
    pub placement_id: Option<String>,
    #[serde(rename = "publisherId", skip_serializing_if = "Option::is_none")]
    pub publisher_id: Option<String>,
    #[serde(rename = "mediaId", skip_serializing_if = "Option::is_none")]
    pub media_id: Option<String>,
    #[serde(rename = "accountId", skip_serializing_if = "Option::is_none")]
    pub account_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpUnruly {
    #[serde(rename = "siteid")]
    pub site_id_old: i64,
    #[serde(rename = "siteId")]
    pub site_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpVideoByte {
    #[serde(rename = "pubId")]
    pub publisher_id: String,
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "nid")]
    pub network_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpVideoHeroes {
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpVrtcal {
    #[serde(rename = "Just_an_unused_vrtcal_param")]
    pub just_an_unused_vrtcal_param: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpYahooAds {
    pub dcn: String,
    pub pos: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpYahooAdvertising {
    pub dcn: String,
    pub pos: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpYahooSSP {
    pub dcn: String,
    pub pos: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpYandex {
    pub placement_id: String,
    pub page_id: i64,
    pub imp_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpYeahmobi {
    #[serde(rename = "pubId")]
    pub pub_id: String,
    #[serde(rename = "zoneId")]
    pub zone_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpYieldlab {
    #[serde(rename = "adslotId")]
    pub adslot_id: String,
    #[serde(rename = "supplyId")]
    pub supply_id: String,
    pub targeting: HashMap<String, String>,
    #[serde(rename = "extId")]
    pub ext_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpYieldmo {
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpYieldone {
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpZeroClickFraud {
    #[serde(rename = "sourceId")]
    pub source_id: i64,
    pub host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtImpZmaticoo {
    #[serde(rename = "pubId")]
    pub pub_id: String,
    #[serde(rename = "zoneId")]
    pub zone_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtIntenze {
    #[serde(rename = "accountId")]
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtIQX {
    pub env: String,
    pub pid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtKayzen {
    pub zone: String,
    pub exchange: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtKrushmedia {
    #[serde(rename = "key")]
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtLmKiviads {
    pub env: String,
    pub pid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtMediaGo {
    pub token: String,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtMotorik {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtRelevantDigital {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "pbsHost")]
    pub host: String,
    #[serde(rename = "pbsBufferMs")]
    pub pbs_buffer_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtSilverMob {
    #[serde(rename = "zoneid")]
    pub zone_id: String,
    pub host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtSmartHub {
    #[serde(rename = "partnerName", skip_serializing_if = "Option::is_none")]
    pub partner_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seat: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtSmartyAds {
    #[serde(rename = "accountid")]
    pub account_id: String,
    #[serde(rename = "sourceid")]
    pub source_id: String,
    pub host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtSmrtconnect {
    pub supply_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtUserDataDeviceIdHuaweiAds {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub imei: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub oaid: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gaid: Vec<String>,
    #[serde(rename = "clientTime", skip_serializing_if = "Vec::is_empty")]
    pub client_time: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtUserDataHuaweiAds {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ExtUserDataDeviceIdHuaweiAds>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtXeworks {
    pub env: String,
    pub pid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtAdmatic {
    pub host: String,
    #[serde(rename = "networkId")]
    pub network_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TargetingInline {
    #[serde(rename = "c", skip_serializing_if = "Vec::is_empty")]
    pub category: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(rename = "kv", skip_serializing_if = "HashMap::is_empty")]
    pub key_values: HashMap<String, Vec<String>>,
    #[serde(rename = "auml", skip_serializing_if = "Vec::is_empty")]
    pub ad_unit_matching_label: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtAdnunitus {
    #[serde(rename = "auId")]
    pub auid: String,
    #[serde(rename = "noCookies")]
    pub no_cookies: bool,
    #[serde(rename = "maxDeals")]
    pub max_deals: i64,
    pub network: String,
    #[serde(rename = "bidType", skip_serializing_if = "Option::is_none")]
    pub bid_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targeting: Option<TargetingInline>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtAdQuery {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    pub r#type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtAdTonos {
    #[serde(rename = "supplierId")]
    pub supplier_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtAdverxo {
    #[serde(rename = "adUnitId")]
    pub ad_unit_id: i64,
    pub auth: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtAppush {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtAxis {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtBeyondMedia {
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtBidstack {
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtBidtheatre {
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtBliink {
    #[serde(rename = "tagId")]
    pub tag_id: String,
    #[serde(rename = "imageUrl")]
    pub image_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtBlis {
    #[serde(rename = "spid")]
    pub supply_partner_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtBmtm {
    pub placement_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtBoldwin {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtBoldwinRapid {
    pub pid: String,
    pub tid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtClydo {
    #[serde(rename = "partnerId")]
    pub partner_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtCompass {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtConcert {
    #[serde(rename = "partnerId")]
    pub partner_id: String,
    #[serde(rename = "placementId", skip_serializing_if = "Option::is_none")]
    pub placement_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sizes: Option<Vec<Vec<i64>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtCopper6ssp {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtCWire {
    #[serde(rename = "domainId", skip_serializing_if = "Option::is_none")]
    pub domain_id: Option<i64>,
    #[serde(rename = "placementId", skip_serializing_if = "Option::is_none")]
    pub placement_id: Option<i64>,
    #[serde(rename = "pageId", skip_serializing_if = "Option::is_none")]
    pub page_id: Option<i64>,
    #[serde(rename = "cwcreative", skip_serializing_if = "Option::is_none")]
    pub cw_creative: Option<String>,
    #[serde(rename = "cwdebug", skip_serializing_if = "Option::is_none")]
    pub cw_debug: Option<bool>,
    #[serde(rename = "cwfeatures", skip_serializing_if = "Vec::is_empty")]
    pub cw_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtDefinemedia {
    #[serde(rename = "mandantId")]
    pub mandant_id: i64,
    #[serde(rename = "adslotId")]
    pub adslot_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtEdge226 {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtEmtv {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtEpom {
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtExco {
    #[serde(rename = "tagId")]
    pub tag_id: String,
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtFlatads {
    pub token: String,
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtFlipp {
    #[serde(rename = "publisherNameIdentifier")]
    pub publisher_name_identifier: String,
    #[serde(rename = "creativeType")]
    pub creative_type: String,
    #[serde(rename = "siteId")]
    pub site_id: i64,
    #[serde(rename = "zoneIds", skip_serializing_if = "Vec::is_empty")]
    pub zone_ids: Vec<i64>,
    #[serde(rename = "userKey", skip_serializing_if = "Option::is_none")]
    pub user_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<ImpExtFlippOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtFlippOptions {
    #[serde(rename = "startCompact", skip_serializing_if = "Option::is_none")]
    pub start_compact: Option<bool>,
    #[serde(rename = "dwellExpand", skip_serializing_if = "Option::is_none")]
    pub dwell_expand: Option<bool>,
    #[serde(rename = "contentCode", skip_serializing_if = "Option::is_none")]
    pub content_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtFreewheelSSP {
    #[serde(rename = "zoneId")]
    pub zone_id: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtFRVRAdn {
    pub publisher_id: String,
    pub ad_unit_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtFWSSP {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_site_section_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtGlobalsun {
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtGoldbach {
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
    #[serde(rename = "slotId")]
    pub slot_id: String,
    #[serde(rename = "customTargeting", skip_serializing_if = "HashMap::is_empty")]
    pub custom_targeting: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtInfytv {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtIQZone {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtKargo {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "adSlotID")]
    pub ad_slot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtKiviads {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtKueez {
    #[serde(rename = "cId")]
    pub connection_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtLemmaDigital {
    #[serde(rename = "pid")]
    pub publisher_id: i64,
    #[serde(rename = "aid")]
    pub ad_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtLimelightDigital {
    pub host: String,
    #[serde(rename = "publisherId")]
    pub publisher_id: serde_json::Number,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtLogan {
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtLoyal {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtMabidder {
    pub ppid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtMediasquare {
    pub owner: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtMeloZen {
    #[serde(rename = "pubId")]
    pub pub_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtMgidX {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtMinuteMedia {
    pub org: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtMobkoi {
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtMsft {
    pub placement_id: i64,
    pub member: i64,
    pub inv_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_smaller_sizes: Option<bool>,
    #[serde(rename = "use_pmt_rule", skip_serializing_if = "Option::is_none")]
    pub use_payment_rule: Option<bool>,
    pub keywords: String,
    pub traffic_source_code: String,
    #[serde(rename = "pubclick")]
    pub pub_click: String,
    pub ext_inv_code: String,
    pub ext_imp_id: String,
    pub banner_frameworks: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtNativery {
    #[serde(rename = "widgetId")]
    pub widget_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtNextMillennium {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    pub placement_id: String,
    #[serde(rename = "adSlots", skip_serializing_if = "Vec::is_empty")]
    pub ad_slots: Vec<String>,
    #[serde(rename = "allowedAds", skip_serializing_if = "Vec::is_empty")]
    pub allowed_ads: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtOgury {
    #[serde(rename = "adUnitId", skip_serializing_if = "Option::is_none")]
    pub ad_unit_id: Option<String>,
    #[serde(rename = "assetKey", skip_serializing_if = "Option::is_none")]
    pub asset_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtOperaads {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtOptidigital {
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "pageTemplate", skip_serializing_if = "Option::is_none")]
    pub page_template: Option<String>,
    #[serde(rename = "divId", skip_serializing_if = "Option::is_none")]
    pub div_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtOraki {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtPangle {
    pub token: String,
    #[serde(rename = "appid", skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(rename = "placementid", skip_serializing_if = "Option::is_none")]
    pub placement_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtPgamSsp {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtPlaydigo {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtPubrise {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtQT {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtReadpeak {
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
    #[serde(rename = "siteId")]
    pub site_id: String,
    pub bidfloor: f64,
    #[serde(rename = "tagId")]
    pub tag_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtResetDigital {
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtRise {
    pub publisher_id: String,
    pub org: String,
    #[serde(rename = "placementId")]
    pub placement_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtRubiconDebug {
    #[serde(rename = "cpmoverride", skip_serializing_if = "Option::is_none")]
    pub cpm_override: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtSeedingAlliance {
    #[serde(rename = "adUnitId")]
    pub ad_unit_id: String,
    #[serde(rename = "seatId")]
    pub seat_id: String,
    #[serde(rename = "accountId")]
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtSeedtag {
    #[serde(rename = "adUnitId")]
    pub ad_unit_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtSilverpush {
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
    #[serde(rename = "bidfloor")]
    pub bid_floor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtSmoot {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtTaboola {
    #[serde(rename = "publisherId")]
    pub publisher_id: String,
    #[serde(rename = "publisherDomain")]
    pub publisher_domain: String,
    #[serde(rename = "bidfloor")]
    pub bid_floor: f64,
    #[serde(rename = "tagid")]
    pub tag_id: String,
    #[serde(rename = "tagId")]
    pub tag_id: String,
    #[serde(rename = "bcat")]
    pub b_cat: Vec<String>,
    #[serde(rename = "badv")]
    pub b_adv: Vec<String>,
    #[serde(rename = "pageType")]
    pub page_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtTeqBlaze {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtVidazoo {
    #[serde(rename = "cId")]
    pub connection_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtVidoomy {
    #[serde(rename = "zoneId")]
    pub zone_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtVisibleMeasures {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "endpointId")]
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtVox {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "imageUrl")]
    pub image_url: String,
    #[serde(rename = "displaySizes")]
    pub display_sizes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtVungle {
    pub bid_token: String,
    #[serde(rename = "app_store_id")]
    pub pub_app_store_id: String,
    #[serde(rename = "placement_reference_id")]
    pub placement_ref_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExtZetaGlobalSsp {
    pub sid: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RubiconVideoParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(rename = "playerHeight", skip_serializing_if = "Option::is_none")]
    pub player_height: Option<serde_json::Number>,
    #[serde(rename = "playerWidth", skip_serializing_if = "Option::is_none")]
    pub player_width: Option<serde_json::Number>,
    #[serde(rename = "size_id", skip_serializing_if = "Option::is_none")]
    pub video_size_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip: Option<i64>,
    #[serde(rename = "skipdelay", skip_serializing_if = "Option::is_none")]
    pub skip_delay: Option<i64>,
}
