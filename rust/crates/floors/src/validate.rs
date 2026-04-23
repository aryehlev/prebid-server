//! Validation helpers for price floors. Ported from `floors/validate.go`.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::types::{
    PriceFloorModelGroup, PriceFloorRules, PriceFloorSchema, SchemaDimension, MODEL_WEIGHT_MAX,
    MODEL_WEIGHT_MIN, RATE_MAX, RATE_MIN,
};

/// Errors raised while parsing or validating floors payloads.
#[derive(Debug, Error, PartialEq)]
pub enum FloorsError {
    #[error("invalid schema dimension provided = '{0}'")]
    InvalidSchemaDimension(String),

    #[error("invalid floor rule = '{0}' for schema fields = {1:?}")]
    InvalidFloorRule(String, Vec<String>),

    #[error("invalid FloorsSchemaVersion = '{0}', supported version 2")]
    InvalidSchemaVersion(i32),

    #[error("invalid SkipRate = '{0}'")]
    InvalidSkipRate(i32),

    #[error("invalid FloorMin = '{0}', value should be >= 0")]
    InvalidFloorMin(f64),

    #[error("no model group present in floors.data")]
    NoModelGroups,

    #[error("invalid model '{model}': too many schema fields ({fields} > {limit})")]
    TooManySchemaFields {
        model: String,
        fields: usize,
        limit: usize,
    },

    #[error("invalid model '{model}': too many rules ({rules} > {limit})")]
    TooManyRules {
        model: String,
        rules: usize,
        limit: usize,
    },

    #[error("invalid model '{model}': ModelWeight out of range ({weight})")]
    InvalidModelWeight { model: String, weight: i32 },

    #[error("invalid model '{model}': Default ({default}) < 0")]
    InvalidDefault { model: String, default: f64 },
}

/// Validate a list of schema dimensions.
pub fn validate_schema_dimensions(fields: &[String]) -> Result<(), FloorsError> {
    for field in fields {
        if SchemaDimension::parse(field).is_none() {
            return Err(FloorsError::InvalidSchemaDimension(field.clone()));
        }
    }
    Ok(())
}

/// Validate rule keys against schema field count and normalise keys to
/// lower case, mutating `rule_values` in place. Returns any validation
/// errors accumulated while processing.
pub fn validate_floor_rules_and_lower_key(
    schema: &PriceFloorSchema,
    delimiter: &str,
    rule_values: &mut BTreeMap<String, f64>,
) -> Vec<FloorsError> {
    let mut errs = Vec::new();
    let keys: Vec<String> = rule_values.keys().cloned().collect();
    for key in keys {
        let parts: Vec<&str> = key.split(delimiter).collect();
        if parts.len() != schema.fields.len() {
            errs.push(FloorsError::InvalidFloorRule(
                key.clone(),
                schema.fields.clone(),
            ));
            rule_values.remove(&key);
            continue;
        }
        let lower = key.to_lowercase();
        if lower != key {
            if let Some(v) = rule_values.remove(&key) {
                rule_values.insert(lower, v);
            }
        }
    }
    errs
}

/// Validate schema version, skip rate and floor min.
pub fn validate_floor_params(extr: &PriceFloorRules) -> Result<(), FloorsError> {
    if let Some(data) = &extr.data {
        if data.floors_schema_version != 0 && data.floors_schema_version != 2 {
            return Err(FloorsError::InvalidSchemaVersion(data.floors_schema_version));
        }
        if data.skip_rate < RATE_MIN || data.skip_rate > RATE_MAX {
            return Err(FloorsError::InvalidSkipRate(data.skip_rate));
        }
    }

    if extr.skip_rate < RATE_MIN || extr.skip_rate > RATE_MAX {
        return Err(FloorsError::InvalidSkipRate(extr.skip_rate));
    }

    if extr.floor_min < 0.0 {
        return Err(FloorsError::InvalidFloorMin(extr.floor_min));
    }

    Ok(())
}

/// Account-level limits applied while choosing valid model groups.
#[derive(Debug, Clone, Default)]
pub struct AccountFloorLimits {
    pub max_schema_dims: usize,
    pub max_rules: usize,
}

/// Validate model groups against account-level limits and drop invalid
/// ones. Returns the list of valid groups and any accumulated errors.
pub fn select_valid_model_groups(
    groups: &[PriceFloorModelGroup],
    limits: &AccountFloorLimits,
) -> (Vec<PriceFloorModelGroup>, Vec<FloorsError>) {
    let mut errs = Vec::new();
    let mut valid = Vec::new();

    if groups.is_empty() {
        errs.push(FloorsError::NoModelGroups);
        return (valid, errs);
    }

    for mg in groups {
        if let Err(e) = validate_schema_dimensions(&mg.schema.fields) {
            errs.push(e);
            continue;
        }
        if limits.max_schema_dims > 0 && mg.schema.fields.len() > limits.max_schema_dims {
            errs.push(FloorsError::TooManySchemaFields {
                model: mg.model_version.clone(),
                fields: mg.schema.fields.len(),
                limit: limits.max_schema_dims,
            });
            continue;
        }
        if limits.max_rules > 0 && mg.values.len() > limits.max_rules {
            errs.push(FloorsError::TooManyRules {
                model: mg.model_version.clone(),
                rules: mg.values.len(),
                limit: limits.max_rules,
            });
            continue;
        }
        if mg.skip_rate < RATE_MIN || mg.skip_rate > RATE_MAX {
            errs.push(FloorsError::InvalidSkipRate(mg.skip_rate));
            continue;
        }
        if let Some(weight) = mg.model_weight {
            if !(MODEL_WEIGHT_MIN..=MODEL_WEIGHT_MAX).contains(&weight) {
                errs.push(FloorsError::InvalidModelWeight {
                    model: mg.model_version.clone(),
                    weight,
                });
                continue;
            }
        }
        if mg.default < 0.0 {
            errs.push(FloorsError::InvalidDefault {
                model: mg.model_version.clone(),
                default: mg.default,
            });
            continue;
        }
        valid.push(mg.clone());
    }

    (valid, errs)
}

/// Convenience top-level validator: runs schema/dimension/param checks.
pub fn validate(rules: &PriceFloorRules) -> Result<(), FloorsError> {
    validate_floor_params(rules)?;
    if let Some(data) = &rules.data {
        for mg in &data.model_groups {
            validate_schema_dimensions(&mg.schema.fields)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PriceFloorData, PriceFloorModelGroup, PriceFloorSchema};

    fn mg(fields: Vec<&str>, values: &[(&str, f64)]) -> PriceFloorModelGroup {
        let mut v = BTreeMap::new();
        for (k, x) in values {
            v.insert((*k).to_string(), *x);
        }
        PriceFloorModelGroup {
            schema: PriceFloorSchema {
                fields: fields.into_iter().map(|s| s.to_string()).collect(),
                delimiter: "|".to_string(),
            },
            values: v,
            ..Default::default()
        }
    }

    #[test]
    fn schema_dimensions_valid() {
        assert!(validate_schema_dimensions(&["mediaType".into(), "country".into()]).is_ok());
    }

    #[test]
    fn schema_dimensions_invalid() {
        let err = validate_schema_dimensions(&["bogus".into()]).unwrap_err();
        assert!(matches!(err, FloorsError::InvalidSchemaDimension(_)));
    }

    #[test]
    fn floor_rules_normalised_and_invalid_dropped() {
        let schema = PriceFloorSchema {
            fields: vec!["mediaType".into(), "country".into()],
            delimiter: "|".into(),
        };
        let mut values = BTreeMap::new();
        values.insert("Banner|US".into(), 1.1);
        values.insert("video".into(), 0.9); // too few parts, should be dropped
        let errs = validate_floor_rules_and_lower_key(&schema, "|", &mut values);
        assert_eq!(errs.len(), 1);
        assert!(values.contains_key("banner|us"));
        assert!(!values.contains_key("video"));
    }

    #[test]
    fn validate_floor_params_bad_skip_rate() {
        let rules = PriceFloorRules {
            skip_rate: 500,
            ..Default::default()
        };
        assert!(matches!(
            validate_floor_params(&rules),
            Err(FloorsError::InvalidSkipRate(500))
        ));
    }

    #[test]
    fn validate_floor_params_bad_floor_min() {
        let rules = PriceFloorRules {
            floor_min: -1.0,
            ..Default::default()
        };
        assert!(matches!(
            validate_floor_params(&rules),
            Err(FloorsError::InvalidFloorMin(_))
        ));
    }

    #[test]
    fn select_valid_model_groups_drops_invalid() {
        let groups = vec![
            mg(vec!["mediaType"], &[("banner", 1.0)]),
            mg(vec!["bogus"], &[("x", 1.0)]),
        ];
        let limits = AccountFloorLimits::default();
        let (valid, errs) = select_valid_model_groups(&groups, &limits);
        assert_eq!(valid.len(), 1);
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn validate_end_to_end() {
        let rules = PriceFloorRules {
            floor_min: 1.0,
            data: Some(PriceFloorData {
                model_groups: vec![mg(vec!["mediaType"], &[("banner", 1.0)])],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(validate(&rules).is_ok());
    }
}
