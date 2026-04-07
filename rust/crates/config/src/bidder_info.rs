//! Bidder info logic ported from Go `config/bidderinfo.go`.
//!
//! This module adds methods and type aliases to the existing `BidderInfo`
//! struct defined in `lib.rs`: enabled checks, media type queries, alias
//! processing, GVL vendor ID mapping, and validation.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::{BidderInfo, BidderSyncerConfig, CapabilitiesInfo, PlatformInfo};

// ---------------------------------------------------------------------------
// BidderInfos type alias
// ---------------------------------------------------------------------------

/// A map of bidder name to [`BidderInfo`].
///
/// Maps to the Go `BidderInfos` type in config/bidderinfo.go.
pub type BidderInfos = HashMap<String, BidderInfo>;

// ---------------------------------------------------------------------------
// MediaType enum
// ---------------------------------------------------------------------------

/// Supported media types for bidder capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaType {
    Banner,
    Video,
    Native,
    Audio,
}

impl MediaType {
    /// Parse a media type string (case-insensitive).
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "banner" => Some(MediaType::Banner),
            "video" => Some(MediaType::Video),
            "native" => Some(MediaType::Native),
            "audio" => Some(MediaType::Audio),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            MediaType::Banner => "banner",
            MediaType::Video => "video",
            MediaType::Native => "native",
            MediaType::Audio => "audio",
        }
    }
}

// ---------------------------------------------------------------------------
// BidderInfo methods
// ---------------------------------------------------------------------------

impl BidderInfo {
    /// Whether this bidder is enabled (not disabled and not white-label-only).
    ///
    /// Maps to Go `BidderInfo.IsEnabled()`.
    pub fn is_enabled(&self) -> bool {
        !self.disabled
    }

    /// Whether this bidder supports the given media type on a specific platform.
    ///
    /// `platform` should be "app", "site", or "dooh".
    ///
    /// Maps to checking capabilities.{platform}.mediaTypes in Go.
    pub fn supports_media_type(&self, platform: &str, media_type: &str) -> bool {
        let caps = match &self.capabilities {
            Some(c) => c,
            None => return false,
        };

        let platform_info = match platform.to_lowercase().as_str() {
            "app" => caps.app.as_ref(),
            "site" | "web" => caps.site.as_ref(),
            "dooh" => caps.dooh.as_ref(),
            _ => None,
        };

        match platform_info {
            Some(pi) => pi
                .media_types
                .iter()
                .any(|mt| mt.eq_ignore_ascii_case(media_type)),
            None => false,
        }
    }

    /// Whether this bidder's syncer config is defined (has meaningful fields).
    ///
    /// Maps to Go `Syncer.Defined()`.
    pub fn syncer_defined(&self) -> bool {
        match &self.user_sync {
            None => false,
            Some(s) => {
                !s.key.is_empty()
                    || s.iframe.is_some()
                    || s.redirect.is_some()
                    || !s.external_url.is_empty()
                    || !s.format_override.is_empty()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// BidderInfos functions
// ---------------------------------------------------------------------------

/// Transform a BidderInfos map to a map of bidder name -> GVL vendor ID.
///
/// Disabled bidders are omitted.
///
/// Maps to Go `BidderInfos.ToGVLVendorIDMap()`.
pub fn to_gvl_vendor_id_map(infos: &BidderInfos) -> HashMap<String, u16> {
    let mut result = HashMap::new();
    for (name, info) in infos {
        if info.is_enabled() && info.gvl_vendor_id != 0 {
            result.insert(name.clone(), info.gvl_vendor_id);
        }
    }
    result
}

/// Load bidder info YAML files from a directory and process aliases.
///
/// This is an enhanced version of the existing `load_bidder_info` function
/// that also resolves alias inheritance (inheriting fields from the parent
/// bidder).
///
/// Maps to Go `LoadBidderInfoFromDisk()` + `processBidderInfos()` +
/// `processBidderAliases()`.
pub fn load_bidder_info_from_disk(dir: &str) -> Result<BidderInfos> {
    let dir_path = Path::new(dir);
    if !dir_path.is_dir() {
        bail!(
            "bidder info directory '{}' does not exist or is not a directory",
            dir
        );
    }

    let mut bidders = BidderInfos::new();
    let mut alias_bidders = Vec::new();

    let entries = std::fs::read_dir(dir_path)
        .with_context(|| format!("failed to read bidder info directory '{}'", dir))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "yaml" && ext != "yml" {
            continue;
        }

        let bidder_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        if bidder_name.is_empty() {
            continue;
        }

        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read bidder info file '{}'", path.display()))?;
        let info: BidderInfo = serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse bidder info file '{}'", path.display()))?;

        if !info.alias_of.is_empty() {
            alias_bidders.push((bidder_name.clone(), info.clone()));
        }

        bidders.insert(bidder_name, info);
    }

    // Process aliases: inherit fields from parent
    process_bidder_aliases(&mut bidders, &alias_bidders)?;

    Ok(bidders)
}

/// Process bidder aliases, inheriting fields from parent bidders.
///
/// Maps to Go `processBidderAliases()`.
fn process_bidder_aliases(
    bidders: &mut BidderInfos,
    alias_bidders: &[(String, BidderInfo)],
) -> Result<()> {
    for (alias_name, _alias_info) in alias_bidders {
        let alias_info = bidders
            .get(alias_name)
            .ok_or_else(|| anyhow::anyhow!("bidder info not found for alias: {}", alias_name))?
            .clone();

        let parent_name = &alias_info.alias_of;

        // Validate alias
        validate_alias(&alias_info, bidders, alias_name)?;

        let parent = bidders
            .get(parent_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "alias '{}' references nonexistent bidder '{}'",
                    alias_name,
                    parent_name
                )
            })?
            .clone();

        let mut merged = alias_info.clone();

        // Inherit fields from parent when not set on alias
        if merged.app_secret.is_empty() {
            merged.app_secret = parent.app_secret.clone();
        }
        if merged.capabilities.is_none() {
            merged.capabilities = parent.capabilities.clone();
        }
        if merged.debug.is_none() {
            merged.debug = parent.debug.clone();
        }
        if merged.endpoint.is_empty() {
            merged.endpoint = parent.endpoint.clone();
        }
        if merged.endpoint_compression.is_empty() {
            merged.endpoint_compression = parent.endpoint_compression.clone();
        }
        if merged.extra_info.is_empty() {
            merged.extra_info = parent.extra_info.clone();
        }
        if merged.maintainer.is_none() {
            merged.maintainer = parent.maintainer.clone();
        }
        if merged.openrtb.is_none() {
            merged.openrtb = parent.openrtb.clone();
        }
        if merged.platform_id.is_empty() {
            merged.platform_id = parent.platform_id.clone();
        }
        if merged.user_sync.is_none() && parent_syncer_defined(&parent) {
            let syncer_key = if let Some(ref ps) = parent.user_sync {
                if !ps.key.is_empty() {
                    ps.key.clone()
                } else {
                    parent_name.clone()
                }
            } else {
                parent_name.clone()
            };
            merged.user_sync = Some(BidderSyncerConfig {
                key: syncer_key,
                ..Default::default()
            });
        }
        if merged.xapi.username.is_empty() {
            merged.xapi = parent.xapi.clone();
        }

        bidders.insert(alias_name.clone(), merged);
    }

    Ok(())
}

fn parent_syncer_defined(parent: &BidderInfo) -> bool {
    match &parent.user_sync {
        None => false,
        Some(s) => {
            !s.key.is_empty()
                || s.iframe.is_some()
                || s.redirect.is_some()
                || !s.external_url.is_empty()
                || !s.format_override.is_empty()
        }
    }
}

/// Validate an alias bidder.
///
/// Maps to Go `validateAliases()`.
fn validate_alias(alias: &BidderInfo, infos: &BidderInfos, alias_name: &str) -> Result<()> {
    if alias.alias_of.is_empty() {
        return Ok(());
    }

    let parent = infos.get(&alias.alias_of);
    match parent {
        None => bail!(
            "alias '{}' references a nonexistent bidder '{}'",
            alias_name,
            alias.alias_of
        ),
        Some(p) => {
            if !p.alias_of.is_empty() {
                bail!(
                    "alias '{}' cannot reference another alias '{}'",
                    alias_name,
                    alias.alias_of
                );
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Validate a list of bidder infos, returning all errors found.
///
/// Maps to Go `BidderInfos.validate()`.
pub fn validate_bidder_infos(infos: &BidderInfos) -> Vec<String> {
    let mut errs = Vec::new();
    for (name, bidder) in infos {
        if bidder.is_enabled() {
            if bidder.endpoint.is_empty() {
                errs.push(format!(
                    "There's no default endpoint available for {}. \
                     Calls to this bidder/exchange will fail. \
                     Please set adapters.{}.endpoint in your app config",
                    name, name
                ));
            }

            if let Err(e) = validate_bidder_capabilities(&bidder.capabilities, name) {
                errs.push(e);
            }

            if let Err(e) = validate_bidder_maintainer(&bidder.maintainer, name) {
                errs.push(e);
            }

            if let Err(e) = validate_syncer_config(bidder) {
                errs.push(e);
            }
        }
    }
    errs
}

/// Validate capabilities for a bidder.
///
/// Maps to Go `validateCapabilities()`.
fn validate_bidder_capabilities(
    caps: &Option<CapabilitiesInfo>,
    bidder_name: &str,
) -> Result<(), String> {
    let caps = match caps {
        Some(c) => c,
        None => {
            return Err(format!(
                "missing required field: capabilities for adapter: {}",
                bidder_name
            ))
        }
    };

    if caps.app.is_none() && caps.site.is_none() && caps.dooh.is_none() {
        return Err(format!(
            "at least one of capabilities.site, capabilities.app, or \
             capabilities.dooh must exist for adapter: {}",
            bidder_name
        ));
    }

    if let Some(ref app) = caps.app {
        validate_platform_info(app, "app", bidder_name)?;
    }
    if let Some(ref site) = caps.site {
        validate_platform_info(site, "site", bidder_name)?;
    }
    if let Some(ref dooh) = caps.dooh {
        validate_platform_info(dooh, "dooh", bidder_name)?;
    }

    Ok(())
}

/// Validate platform info media types.
///
/// Maps to Go `validatePlatformInfo()`.
fn validate_platform_info(
    info: &PlatformInfo,
    platform: &str,
    bidder_name: &str,
) -> Result<(), String> {
    if info.media_types.is_empty() {
        return Err(format!(
            "capabilities.{} failed validation: at least one media type \
             needs to be specified for adapter: {}",
            platform, bidder_name
        ));
    }

    for (i, mt) in info.media_types.iter().enumerate() {
        if mt != "banner" && mt != "video" && mt != "native" && mt != "audio" {
            return Err(format!(
                "capabilities.{} failed validation: unrecognized media type \
                 at index {}: {} for adapter: {}",
                platform, i, mt, bidder_name
            ));
        }
    }

    Ok(())
}

/// Validate maintainer info for a bidder.
fn validate_bidder_maintainer(
    maintainer: &Option<crate::MaintainerInfo>,
    bidder_name: &str,
) -> Result<(), String> {
    match maintainer {
        Some(m) if !m.email.is_empty() => Ok(()),
        _ => Err(format!(
            "missing required field: maintainer.email for adapter: {}",
            bidder_name
        )),
    }
}

/// Validate syncer config for a bidder.
///
/// Maps to Go `validateSyncer()`.
fn validate_syncer_config(bidder: &BidderInfo) -> Result<(), String> {
    let syncer = match &bidder.user_sync {
        Some(s) => s,
        None => return Ok(()),
    };

    if !syncer.format_override.is_empty()
        && syncer.format_override != "b"
        && syncer.format_override != "i"
    {
        return Err(format!(
            "syncer could not be created, invalid format override value: {}",
            syncer.format_override
        ));
    }

    for s in &syncer.supports {
        if !s.eq_ignore_ascii_case("iframe") && !s.eq_ignore_ascii_case("redirect") {
            return Err(format!(
                "syncer could not be created, invalid supported endpoint: {}",
                s
            ));
        }
    }

    Ok(())
}

/// Validate geoscope entries for a bidder.
///
/// Maps to Go `validateGeoscope()`.
pub fn validate_geoscope(geoscope: &[String], bidder_name: &str) -> Result<(), String> {
    for (i, code_raw) in geoscope.iter().enumerate() {
        let code = code_raw.trim().to_uppercase();

        if code == "GLOBAL" || code == "EEA" {
            continue;
        }

        let check = if code.starts_with('!') {
            &code[1..]
        } else {
            &code
        };

        if check.len() != 3 {
            return Err(format!(
                "invalid geoscope entry at index {}: {} for adapter: {} - \
                 must be a 3-letter ISO 3166-1 alpha-3 country code",
                i, code_raw, bidder_name
            ));
        }

        for ch in check.chars() {
            if !ch.is_ascii_uppercase() {
                return Err(format!(
                    "invalid geoscope entry at index {}: {} for adapter: {} - \
                     must contain only uppercase letters A-Z",
                    i, code_raw, bidder_name
                ));
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BidderInfo, BidderSyncerConfig, CapabilitiesInfo, MaintainerInfo,
        PlatformInfo,
    };

    // -- BidderInfo::is_enabled --

    #[test]
    fn test_is_enabled_default() {
        let bi = BidderInfo::default();
        assert!(bi.is_enabled());
    }

    #[test]
    fn test_is_enabled_disabled() {
        let bi = BidderInfo {
            disabled: true,
            ..Default::default()
        };
        assert!(!bi.is_enabled());
    }

    // -- supports_media_type --

    #[test]
    fn test_supports_media_type_banner_on_site() {
        let bi = BidderInfo {
            capabilities: Some(CapabilitiesInfo {
                site: Some(PlatformInfo {
                    media_types: vec!["banner".to_string(), "video".to_string()],
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(bi.supports_media_type("site", "banner"));
        assert!(bi.supports_media_type("site", "video"));
        assert!(!bi.supports_media_type("site", "native"));
        assert!(!bi.supports_media_type("app", "banner"));
    }

    #[test]
    fn test_supports_media_type_no_capabilities() {
        let bi = BidderInfo::default();
        assert!(!bi.supports_media_type("site", "banner"));
    }

    // -- syncer_defined --

    #[test]
    fn test_syncer_defined_none() {
        let bi = BidderInfo::default();
        assert!(!bi.syncer_defined());
    }

    #[test]
    fn test_syncer_defined_with_key() {
        let bi = BidderInfo {
            user_sync: Some(BidderSyncerConfig {
                key: "mykey".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(bi.syncer_defined());
    }

    #[test]
    fn test_syncer_defined_empty() {
        let bi = BidderInfo {
            user_sync: Some(BidderSyncerConfig::default()),
            ..Default::default()
        };
        assert!(!bi.syncer_defined());
    }

    // -- to_gvl_vendor_id_map --

    #[test]
    fn test_to_gvl_vendor_id_map() {
        let mut infos = BidderInfos::new();
        infos.insert(
            "bidderA".to_string(),
            BidderInfo {
                gvl_vendor_id: 42,
                ..Default::default()
            },
        );
        infos.insert(
            "bidderB".to_string(),
            BidderInfo {
                gvl_vendor_id: 0,
                ..Default::default()
            },
        );
        infos.insert(
            "bidderC".to_string(),
            BidderInfo {
                disabled: true,
                gvl_vendor_id: 99,
                ..Default::default()
            },
        );

        let map = to_gvl_vendor_id_map(&infos);
        assert_eq!(map.len(), 1);
        assert_eq!(map["bidderA"], 42);
    }

    // -- validate_bidder_infos --

    #[test]
    fn test_validate_bidder_infos_missing_endpoint() {
        let mut infos = BidderInfos::new();
        infos.insert(
            "test".to_string(),
            BidderInfo {
                endpoint: "".to_string(),
                maintainer: Some(MaintainerInfo {
                    email: "test@example.com".to_string(),
                }),
                capabilities: Some(CapabilitiesInfo {
                    site: Some(PlatformInfo {
                        media_types: vec!["banner".to_string()],
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );

        let errs = validate_bidder_infos(&infos);
        assert!(errs.iter().any(|e| e.contains("no default endpoint")));
    }

    #[test]
    fn test_validate_bidder_infos_missing_capabilities() {
        let mut infos = BidderInfos::new();
        infos.insert(
            "test".to_string(),
            BidderInfo {
                endpoint: "https://example.com".to_string(),
                maintainer: Some(MaintainerInfo {
                    email: "test@example.com".to_string(),
                }),
                capabilities: None,
                ..Default::default()
            },
        );

        let errs = validate_bidder_infos(&infos);
        assert!(errs.iter().any(|e| e.contains("capabilities")));
    }

    #[test]
    fn test_validate_bidder_infos_missing_maintainer() {
        let mut infos = BidderInfos::new();
        infos.insert(
            "test".to_string(),
            BidderInfo {
                endpoint: "https://example.com".to_string(),
                maintainer: None,
                capabilities: Some(CapabilitiesInfo {
                    site: Some(PlatformInfo {
                        media_types: vec!["banner".to_string()],
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );

        let errs = validate_bidder_infos(&infos);
        assert!(errs.iter().any(|e| e.contains("maintainer.email")));
    }

    #[test]
    fn test_validate_bidder_infos_valid() {
        let mut infos = BidderInfos::new();
        infos.insert(
            "test".to_string(),
            BidderInfo {
                endpoint: "https://example.com".to_string(),
                maintainer: Some(MaintainerInfo {
                    email: "test@example.com".to_string(),
                }),
                capabilities: Some(CapabilitiesInfo {
                    site: Some(PlatformInfo {
                        media_types: vec!["banner".to_string()],
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );

        let errs = validate_bidder_infos(&infos);
        assert!(errs.is_empty(), "expected no errors, got: {:?}", errs);
    }

    #[test]
    fn test_validate_bidder_infos_disabled_skipped() {
        let mut infos = BidderInfos::new();
        infos.insert(
            "test".to_string(),
            BidderInfo {
                disabled: true,
                // Missing everything, but disabled so should pass
                ..Default::default()
            },
        );

        let errs = validate_bidder_infos(&infos);
        assert!(errs.is_empty());
    }

    // -- validate_syncer_config --

    #[test]
    fn test_validate_syncer_invalid_format_override() {
        let bi = BidderInfo {
            user_sync: Some(BidderSyncerConfig {
                format_override: "x".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = validate_syncer_config(&bi);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid format override"));
    }

    #[test]
    fn test_validate_syncer_valid_format_overrides() {
        for fmt in &["b", "i", ""] {
            let bi = BidderInfo {
                user_sync: Some(BidderSyncerConfig {
                    format_override: fmt.to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            };
            assert!(validate_syncer_config(&bi).is_ok());
        }
    }

    #[test]
    fn test_validate_syncer_invalid_supports() {
        let bi = BidderInfo {
            user_sync: Some(BidderSyncerConfig {
                supports: vec!["badtype".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = validate_syncer_config(&bi);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid supported endpoint"));
    }

    #[test]
    fn test_validate_syncer_valid_supports() {
        let bi = BidderInfo {
            user_sync: Some(BidderSyncerConfig {
                supports: vec!["iframe".to_string(), "redirect".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(validate_syncer_config(&bi).is_ok());
    }

    // -- validate_geoscope --

    #[test]
    fn test_validate_geoscope_valid() {
        let scopes = vec!["USA".to_string(), "GBR".to_string(), "GLOBAL".to_string()];
        assert!(validate_geoscope(&scopes, "test").is_ok());
    }

    #[test]
    fn test_validate_geoscope_eea() {
        let scopes = vec!["EEA".to_string()];
        assert!(validate_geoscope(&scopes, "test").is_ok());
    }

    #[test]
    fn test_validate_geoscope_exclusion() {
        let scopes = vec!["!USA".to_string()];
        assert!(validate_geoscope(&scopes, "test").is_ok());
    }

    #[test]
    fn test_validate_geoscope_invalid_length() {
        let scopes = vec!["US".to_string()];
        let result = validate_geoscope(&scopes, "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("3-letter"));
    }

    #[test]
    fn test_validate_geoscope_invalid_chars() {
        let scopes = vec!["U1A".to_string()];
        let result = validate_geoscope(&scopes, "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("uppercase letters"));
    }

    // -- MediaType --

    #[test]
    fn test_media_type_parsing() {
        assert_eq!(MediaType::from_str_opt("banner"), Some(MediaType::Banner));
        assert_eq!(MediaType::from_str_opt("VIDEO"), Some(MediaType::Video));
        assert_eq!(MediaType::from_str_opt("Native"), Some(MediaType::Native));
        assert_eq!(MediaType::from_str_opt("audio"), Some(MediaType::Audio));
        assert_eq!(MediaType::from_str_opt("unknown"), None);
    }

    #[test]
    fn test_media_type_as_str() {
        assert_eq!(MediaType::Banner.as_str(), "banner");
        assert_eq!(MediaType::Video.as_str(), "video");
        assert_eq!(MediaType::Native.as_str(), "native");
        assert_eq!(MediaType::Audio.as_str(), "audio");
    }

    // -- Alias validation --

    #[test]
    fn test_validate_alias_nonexistent_parent() {
        let alias = BidderInfo {
            alias_of: "nonexistent".to_string(),
            ..Default::default()
        };
        let infos = BidderInfos::new();
        let result = validate_alias(&alias, &infos, "myalias");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("nonexistent bidder"));
    }

    #[test]
    fn test_validate_alias_chain() {
        let mut infos = BidderInfos::new();
        infos.insert(
            "parent".to_string(),
            BidderInfo {
                alias_of: "grandparent".to_string(),
                ..Default::default()
            },
        );

        let alias = BidderInfo {
            alias_of: "parent".to_string(),
            ..Default::default()
        };
        let result = validate_alias(&alias, &infos, "child");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot reference another alias"));
    }

    #[test]
    fn test_validate_alias_valid() {
        let mut infos = BidderInfos::new();
        infos.insert("parent".to_string(), BidderInfo::default());

        let alias = BidderInfo {
            alias_of: "parent".to_string(),
            ..Default::default()
        };
        assert!(validate_alias(&alias, &infos, "child").is_ok());
    }

    // -- Platform info validation --

    #[test]
    fn test_validate_platform_info_empty() {
        let pi = PlatformInfo {
            media_types: vec![],
        };
        let result = validate_platform_info(&pi, "site", "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least one media type"));
    }

    #[test]
    fn test_validate_platform_info_unrecognized() {
        let pi = PlatformInfo {
            media_types: vec!["banner".to_string(), "unknown".to_string()],
        };
        let result = validate_platform_info(&pi, "app", "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unrecognized media type"));
    }

    #[test]
    fn test_validate_platform_info_valid() {
        let pi = PlatformInfo {
            media_types: vec!["banner".to_string(), "video".to_string()],
        };
        assert!(validate_platform_info(&pi, "site", "test").is_ok());
    }
}
