//! Static bidder-info YAML loaders.
//!
//! These helpers parse bidder YAML files on disk or from strings into the
//! existing [`crate::BidderInfo`] struct. They are intentionally standalone
//! and do not perform any alias post-processing — that lives in
//! `bidder_info::load_bidder_info_from_disk`.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use thiserror::Error;

use crate::BidderInfo;

/// Errors that can occur while loading static bidder configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io error reading '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse bidder info YAML for '{bidder}': {source}")]
    Yaml {
        bidder: String,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("failed to parse bidder params JSON for '{bidder}': {source}")]
    Json {
        bidder: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("failed to compile JSON schema for '{bidder}': {message}")]
    SchemaCompile { bidder: String, message: String },

    #[error("path '{path}' is not a directory")]
    NotADirectory { path: String },
}

/// Parse a single bidder info YAML blob.
pub fn load_bidder_info_yaml(
    yaml_text: &str,
    bidder_name: &str,
) -> Result<BidderInfo, ConfigError> {
    serde_yaml::from_str::<BidderInfo>(yaml_text).map_err(|e| ConfigError::Yaml {
        bidder: bidder_name.to_string(),
        source: e,
    })
}

/// Walk a directory of `*.yaml` bidder info files and parse each into a
/// [`BidderInfo`]. The returned map is keyed by the filename stem (e.g.
/// `appnexus.yaml` -> `"appnexus"`).
pub fn load_bidder_infos_from_dir(
    path: &Path,
) -> Result<HashMap<String, BidderInfo>, ConfigError> {
    if !path.is_dir() {
        return Err(ConfigError::NotADirectory {
            path: path.display().to_string(),
        });
    }

    let mut out = HashMap::new();

    let entries = fs::read_dir(path).map_err(|e| ConfigError::Io {
        path: path.display().to_string(),
        source: e,
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| ConfigError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let entry_path = entry.path();

        let ext = entry_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if ext != "yaml" && ext != "yml" {
            continue;
        }

        let stem = match entry_path.file_stem().and_then(|s| s.to_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };

        let contents = fs::read_to_string(&entry_path).map_err(|e| ConfigError::Io {
            path: entry_path.display().to_string(),
            source: e,
        })?;

        let info = load_bidder_info_yaml(&contents, &stem)?;
        out.insert(stem, info);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const CANNED_YAML: &str = r#"
endpoint: "https://example.com/openrtb2"
maintainer:
  email: "test@example.com"
gvlVendorID: 42
capabilities:
  site:
    mediaTypes:
      - banner
      - video
"#;

    #[test]
    fn test_load_bidder_info_yaml_parses_canned() {
        let info = load_bidder_info_yaml(CANNED_YAML, "canned").unwrap();
        assert_eq!(info.endpoint, "https://example.com/openrtb2");
        assert_eq!(info.gvl_vendor_id, 42);
        let maintainer = info.maintainer.as_ref().unwrap();
        assert_eq!(maintainer.email, "test@example.com");
        let caps = info.capabilities.as_ref().unwrap();
        let site = caps.site.as_ref().unwrap();
        assert_eq!(site.media_types, vec!["banner", "video"]);
    }

    #[test]
    fn test_load_bidder_info_yaml_bad() {
        let err = load_bidder_info_yaml("endpoint: [broken", "bad");
        assert!(err.is_err());
    }

    #[test]
    fn test_load_bidder_infos_from_dir_two_files() {
        let dir = tempdir().unwrap();

        let a_path = dir.path().join("alpha.yaml");
        fs::write(
            &a_path,
            r#"endpoint: "https://a.example.com"
maintainer:
  email: "a@example.com"
capabilities:
  site:
    mediaTypes:
      - banner
"#,
        )
        .unwrap();

        let b_path = dir.path().join("beta.yaml");
        fs::write(
            &b_path,
            r#"endpoint: "https://b.example.com"
gvlVendorID: 7
maintainer:
  email: "b@example.com"
capabilities:
  app:
    mediaTypes:
      - video
"#,
        )
        .unwrap();

        // Non-yaml file should be ignored.
        fs::write(dir.path().join("README.txt"), "ignore me").unwrap();

        let map = load_bidder_infos_from_dir(dir.path()).unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map["alpha"].endpoint, "https://a.example.com");
        assert_eq!(map["beta"].endpoint, "https://b.example.com");
        assert_eq!(map["beta"].gvl_vendor_id, 7);
    }

    #[test]
    fn test_load_bidder_infos_from_dir_missing() {
        let err = load_bidder_infos_from_dir(Path::new("/definitely/not/a/real/path/xyz"));
        assert!(err.is_err());
    }
}
