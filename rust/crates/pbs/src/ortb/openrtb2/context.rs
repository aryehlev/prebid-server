use serde::{Deserialize, Serialize};

use crate::ortb::adcom1::{
    CategoryTaxonomy, ContentContext, DoohVenueTaxonomy, MediaRating, ProductionQuality,
};
use crate::ortb::{de, Ext};

/// Website (§3.2.13).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Site {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub id: String,
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
    #[serde(skip_serializing_if = "de::is_zero")]
    pub cattax: CategoryTaxonomy,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub cat: Vec<String>,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub sectioncat: Vec<String>,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub pagecat: Vec<String>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub page: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub r#ref: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub search: String,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub mobile: Option<i8>,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub privacypolicy: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Publisher>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
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
    pub inventorypartnerdomain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Application (§3.2.14).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct App {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub id: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub name: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub bundle: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub domain: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub storeurl: String,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub cattax: CategoryTaxonomy,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub cat: Vec<String>,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub sectioncat: Vec<String>,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub pagecat: Vec<String>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub ver: String,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub privacypolicy: Option<i8>,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub paid: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Publisher>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
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
    pub inventorypartnerdomain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Digital out-of-home placement (§3.2.30).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Dooh {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub id: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub name: String,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub venuetype: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub venuetypetax: Option<DoohVenueTaxonomy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Publisher>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub domain: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub keywords: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Publisher (§3.2.15).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Publisher {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub id: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub name: String,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub cattax: CategoryTaxonomy,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub cat: Vec<String>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub domain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Content (§3.2.16).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Content {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub id: String,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub episode: i64,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub title: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub series: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub season: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub artist: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub genre: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub album: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub isrc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer: Option<Producer>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub url: String,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub cattax: CategoryTaxonomy,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub cat: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prodq: Option<ProductionQuality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub videoquality: Option<ProductionQuality>,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub context: ContentContext,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub contentrating: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub userrating: String,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub qagmediarating: MediaRating,
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
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub livestream: Option<i8>,
    #[serde(
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub sourcerelationship: Option<i8>,
    #[serde(deserialize_with = "de::int", skip_serializing_if = "de::is_zero")]
    pub len: i64,
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
        deserialize_with = "de::opt_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub embeddable: Option<i8>,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<Data>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<Network>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<Channel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Content producer (§3.2.17).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Producer {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub id: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub name: String,
    #[serde(skip_serializing_if = "de::is_zero")]
    pub cattax: CategoryTaxonomy,
    #[serde(
        deserialize_with = "de::strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub cat: Vec<String>,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub domain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Network (§3.2.23).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Network {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub id: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Channel (§3.2.24).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Channel {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub id: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Data (§3.2.21).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub id: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub name: String,
    #[serde(deserialize_with = "de::seq", skip_serializing_if = "Vec::is_empty")]
    pub segment: Vec<Segment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}

/// Segment (§3.2.22).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Segment {
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub id: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub name: String,
    #[serde(
        deserialize_with = "de::string",
        skip_serializing_if = "String::is_empty"
    )]
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Ext>,
}
