//! TCF2 Global Vendor List (GVL) data types.
//!
//! Ported in simplified form from the IAB vendor-list schema used by the Go
//! `vendorlist` package. This crate is deliberately minimal - only the
//! shapes required for typical prebid-server use are modeled here.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Numeric purpose identifier from the TCF specification (1-11).
pub type PurposeId = u8;

/// Numeric GVL vendor identifier.
pub type VendorId = u16;

/// A TCF purpose entry as it appears in the GVL JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Purpose {
    pub id: PurposeId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "descriptionLegal", default)]
    pub description_legal: String,
    /// Whether the user consent flag is allowed for this purpose.
    #[serde(rename = "consentable", default = "default_true")]
    pub consentable: bool,
    /// Whether the legitimate-interest flag is allowed for this purpose.
    #[serde(rename = "rightToObject", default = "default_true")]
    pub right_to_object: bool,
}

fn default_true() -> bool {
    true
}

/// A single GVL vendor entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vendor {
    pub id: VendorId,
    pub name: String,

    #[serde(default, rename = "purposes")]
    pub purposes: Vec<PurposeId>,

    #[serde(default, rename = "legIntPurposes")]
    pub leg_int_purposes: Vec<PurposeId>,

    #[serde(default, rename = "flexiblePurposes")]
    pub flexible_purposes: Vec<PurposeId>,

    #[serde(default, rename = "specialPurposes")]
    pub special_purposes: Vec<PurposeId>,

    #[serde(default, rename = "features")]
    pub features: Vec<u8>,

    #[serde(default, rename = "specialFeatures")]
    pub special_features: Vec<u8>,

    #[serde(default, rename = "policyUrl")]
    pub policy_url: String,

    #[serde(default, rename = "deletedDate")]
    pub deleted_date: Option<DateTime<Utc>>,
}

impl Vendor {
    /// Whether this vendor declares the given purpose under the
    /// "consent" legal basis (i.e. it is in the `purposes` list).
    pub fn purpose(&self, id: PurposeId) -> bool {
        self.purposes.contains(&id)
    }

    /// Whether this vendor declares the given purpose under the
    /// "legitimate interest" legal basis.
    pub fn legitimate_interest_purpose(&self, id: PurposeId) -> bool {
        self.leg_int_purposes.contains(&id)
    }

    /// Whether this vendor declares the given purpose as flexible.
    pub fn flexible_purpose(&self, id: PurposeId) -> bool {
        self.flexible_purposes.contains(&id)
    }

    /// Whether this vendor has been deleted (by the given reference time).
    pub fn is_deleted(&self, at: DateTime<Utc>) -> bool {
        matches!(self.deleted_date, Some(d) if d <= at)
    }
}

/// A TCF2 Global Vendor List.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorList {
    #[serde(rename = "gvlSpecificationVersion", default)]
    pub gvl_specification_version: u16,

    #[serde(rename = "vendorListVersion", default)]
    pub vendor_list_version: u16,

    #[serde(rename = "tcfPolicyVersion", default)]
    pub tcf_policy_version: u8,

    #[serde(rename = "lastUpdated", default)]
    pub last_updated: Option<DateTime<Utc>>,

    #[serde(default)]
    pub purposes: BTreeMap<String, Purpose>,

    #[serde(default)]
    pub vendors: BTreeMap<String, Vendor>,
}

impl VendorList {
    /// Returns the vendor with the given id, if any.
    pub fn vendor(&self, id: VendorId) -> Option<&Vendor> {
        self.vendors.get(&id.to_string())
    }

    /// Returns the purpose with the given id, if any.
    pub fn purpose(&self, id: PurposeId) -> Option<&Purpose> {
        self.purposes.get(&id.to_string())
    }

    /// Number of vendors declared in this list.
    pub fn vendor_count(&self) -> usize {
        self.vendors.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_purpose_lookup() {
        let v = Vendor {
            id: 1,
            name: "Test".into(),
            purposes: vec![1, 3, 4],
            leg_int_purposes: vec![7],
            flexible_purposes: vec![3],
            special_purposes: vec![],
            features: vec![],
            special_features: vec![],
            policy_url: String::new(),
            deleted_date: None,
        };
        assert!(v.purpose(1));
        assert!(!v.purpose(2));
        assert!(v.legitimate_interest_purpose(7));
        assert!(v.flexible_purpose(3));
        assert!(!v.is_deleted(Utc::now()));
    }

    #[test]
    fn vendor_list_roundtrip() {
        let json = r#"{
            "gvlSpecificationVersion": 3,
            "vendorListVersion": 42,
            "tcfPolicyVersion": 4,
            "purposes": {
                "1": {"id": 1, "name": "Store information"}
            },
            "vendors": {
                "10": {"id": 10, "name": "Acme", "purposes": [1, 2]}
            }
        }"#;

        let list: VendorList = serde_json::from_str(json).unwrap();
        assert_eq!(list.gvl_specification_version, 3);
        assert_eq!(list.vendor_list_version, 42);
        assert_eq!(list.tcf_policy_version, 4);
        assert_eq!(list.vendor_count(), 1);

        let v = list.vendor(10).expect("vendor 10");
        assert_eq!(v.name, "Acme");
        assert!(v.purpose(1));
        assert!(v.purpose(2));

        let p = list.purpose(1).expect("purpose 1");
        assert_eq!(p.name, "Store information");

        // Round-trip back out to JSON.
        let s = serde_json::to_string(&list).unwrap();
        assert!(s.contains("\"vendorListVersion\":42"));
    }
}
