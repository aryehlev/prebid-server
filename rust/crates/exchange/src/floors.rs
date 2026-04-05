use std::collections::HashMap;
use serde::Deserialize;

/// PriceFloors holds floor configuration from req.ext.prebid.floors
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PriceFloors {
    #[serde(rename = "floorMin", default)]
    pub floor_min: f64,
    #[serde(rename = "floorMinCur", default)]
    pub floor_min_cur: String,
    pub data: Option<FloorData>,
    #[serde(rename = "enforcement", default)]
    pub enforcement: FloorEnforcement,
    #[serde(rename = "skipped", default)]
    pub skipped: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorData {
    pub currency: Option<String>,
    pub schema: Option<FloorSchema>,
    pub values: Option<HashMap<String, f64>>,
    pub modelgroups: Option<Vec<FloorModelGroup>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorSchema {
    pub fields: Vec<String>,
    pub delimiter: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorModelGroup {
    pub schema: Option<FloorSchema>,
    pub values: Option<HashMap<String, f64>>,
    #[serde(rename = "modelWeight", default)]
    pub model_weight: i32,
    #[serde(rename = "skipRate", default)]
    pub skip_rate: i32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorEnforcement {
    #[serde(rename = "enforcePbs", default)]
    pub enforce_pbs: bool,
    #[serde(rename = "floorDeals", default)]
    pub floor_deals: bool,
    #[serde(rename = "enforceRate", default)]
    pub enforce_rate: i32,
}

/// Get the effective floor for an impression from req.ext.prebid.floors
/// Returns the floor price in the request currency (USD by default).
pub fn get_floor_for_imp(
    imp: &openrtb::Imp,
    request: &openrtb::BidRequest,
    floors: &PriceFloors,
) -> Option<f64> {
    if floors.skipped {
        return None;
    }

    // Use explicit imp.bidfloor if set
    if let Some(f) = imp.bidfloor.filter(|&f| f > 0.0) {
        // Apply floorMin
        if floors.floor_min > 0.0 {
            return Some(f.max(floors.floor_min));
        }
        return Some(f);
    }

    // Try schema-based lookup from floors.data
    if let Some(data) = &floors.data {
        if let Some(floor) = lookup_schema_floor(imp, request, data) {
            let min = floors.floor_min;
            return Some(if min > 0.0 { floor.max(min) } else { floor });
        }
    }

    // Fallback to floorMin only
    if floors.floor_min > 0.0 {
        return Some(floors.floor_min);
    }

    None
}

fn lookup_schema_floor(
    imp: &openrtb::Imp,
    request: &openrtb::BidRequest,
    data: &FloorData,
) -> Option<f64> {
    // Try each model group
    let groups = data.modelgroups.as_deref().unwrap_or(&[]);
    for group in groups {
        let schema = group.schema.as_ref().or(data.schema.as_ref())?;
        let values = group.values.as_ref().or(data.values.as_ref())?;
        let delimiter = schema.delimiter.as_deref().unwrap_or("|");

        let key = build_floor_key(&schema.fields, delimiter, imp, request);
        if let Some(&floor) = values.get(&key) {
            return Some(floor);
        }
        // Try wildcard
        let wildcard_key = schema
            .fields
            .iter()
            .map(|_| "*")
            .collect::<Vec<_>>()
            .join(delimiter);
        if let Some(&floor) = values.get(&wildcard_key) {
            return Some(floor);
        }
    }

    // Try top-level values
    if let Some(values) = &data.values {
        if let Some(schema) = &data.schema {
            let delimiter = schema.delimiter.as_deref().unwrap_or("|");
            let key = build_floor_key(&schema.fields, delimiter, imp, request);
            if let Some(&floor) = values.get(&key) {
                return Some(floor);
            }
            // Wildcard fallback for top-level values
            let wildcard_key = schema.fields.iter().map(|_| "*").collect::<Vec<_>>().join(delimiter);
            if let Some(&floor) = values.get(&wildcard_key) {
                return Some(floor);
            }
        }
    }

    None
}

fn build_floor_key(
    fields: &[String],
    delimiter: &str,
    imp: &openrtb::Imp,
    request: &openrtb::BidRequest,
) -> String {
    fields
        .iter()
        .map(|field| match field.as_str() {
            "siteDomain" | "pubDomain" | "domain" => request
                .site
                .as_ref()
                .and_then(|s| s.domain.as_deref())
                .unwrap_or("*")
                .to_string(),
            "bundle" => request
                .app
                .as_ref()
                .and_then(|a| a.bundle.as_deref())
                .unwrap_or("*")
                .to_string(),
            "channel" => request
                .site
                .as_ref()
                .and_then(|s| s.name.as_deref())
                .unwrap_or("*")
                .to_string(),
            "mediaType" => {
                if imp.banner.is_some() {
                    "banner".to_string()
                } else if imp.video.is_some() {
                    "video".to_string()
                } else if imp.native.is_some() {
                    "native".to_string()
                } else {
                    "*".to_string()
                }
            }
            "size" => imp
                .banner
                .as_ref()
                .and_then(|b| b.format.as_ref())
                .and_then(|f| f.first())
                .map(|f| format!("{}x{}", f.w.unwrap_or(0), f.h.unwrap_or(0)))
                .unwrap_or_else(|| "*".to_string()),
            "gptSlot" => imp
                .ext
                .as_ref()
                .and_then(|e| e.get("data"))
                .and_then(|d| d.get("adserver"))
                .and_then(|a| a.get("adslot"))
                .and_then(|v| v.as_str())
                .unwrap_or("*")
                .to_string(),
            _ => "*".to_string(),
        })
        .collect::<Vec<_>>()
        .join(delimiter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_banner_imp(id: &str, bidfloor: Option<f64>) -> openrtb::Imp {
        openrtb::Imp {
            id: id.to_string(),
            bidfloor,
            banner: Some(openrtb::Banner::default()),
            ..Default::default()
        }
    }

    fn make_request() -> openrtb::BidRequest {
        openrtb::BidRequest {
            id: "test".to_string(),
            imp: vec![],
            site: Some(openrtb::Site {
                domain: Some("example.com".to_string()),
                name: Some("mychannel".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_skipped_floors_returns_none() {
        let imp = make_banner_imp("imp1", Some(1.5));
        let req = make_request();
        let floors = PriceFloors {
            skipped: true,
            floor_min: 2.0,
            ..Default::default()
        };
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), None);
    }

    #[test]
    fn test_bidfloor_used_when_set() {
        let imp = make_banner_imp("imp1", Some(1.5));
        let req = make_request();
        let floors = PriceFloors::default();
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(1.5));
    }

    #[test]
    fn test_floor_min_applied_over_bidfloor() {
        let imp = make_banner_imp("imp1", Some(0.5));
        let req = make_request();
        let floors = PriceFloors {
            floor_min: 1.0,
            ..Default::default()
        };
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(1.0));
    }

    #[test]
    fn test_schema_floor_lookup_by_media_type() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();

        let mut values = HashMap::new();
        values.insert("banner".to_string(), 2.5_f64);

        let floors = PriceFloors {
            data: Some(FloorData {
                schema: Some(FloorSchema {
                    fields: vec!["mediaType".to_string()],
                    delimiter: None,
                }),
                values: Some(values),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(2.5));
    }

    #[test]
    fn test_schema_wildcard_fallback() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();

        let mut values = HashMap::new();
        values.insert("*".to_string(), 1.0_f64);

        let floors = PriceFloors {
            data: Some(FloorData {
                schema: Some(FloorSchema {
                    fields: vec!["mediaType".to_string()],
                    delimiter: None,
                }),
                values: Some(values),
                ..Default::default()
            }),
            ..Default::default()
        };
        // "banner" not found, falls through wildcard
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(1.0));
    }

    #[test]
    fn test_floor_min_fallback_when_no_imp_floor() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();
        let floors = PriceFloors {
            floor_min: 0.75,
            ..Default::default()
        };
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(0.75));
    }

    #[test]
    fn test_no_floor_returns_none() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();
        let floors = PriceFloors::default();
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), None);
    }

    #[test]
    fn test_build_floor_key_multi_field() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();
        let fields = vec!["siteDomain".to_string(), "mediaType".to_string()];
        let key = build_floor_key(&fields, "|", &imp, &req);
        assert_eq!(key, "example.com|banner");
    }
}
