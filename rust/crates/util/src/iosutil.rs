//! iOS version parsing and classification — mirrors Go `util/iosutil` package.

/// iOS version with major.minor components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: i32,
    pub minor: i32,
}

impl Version {
    pub fn parse(v: &str) -> Result<Version, String> {
        let parts: Vec<&str> = v.split('.').collect();
        if parts.len() < 2 || parts.len() > 3 {
            return Err("expected either major.minor or major.minor.patch format".to_string());
        }
        let major = parts[0].parse::<i32>().map_err(|_| "major version is not an integer")?;
        let minor = parts[1].parse::<i32>().map_err(|_| "minor version is not an integer")?;
        Ok(Version { major, minor })
    }

    pub fn equal(&self, major: i32, minor: i32) -> bool {
        self.major == major && self.minor == minor
    }

    pub fn equal_or_greater(&self, major: i32, minor: i32) -> bool {
        if self.major == major {
            self.minor >= minor
        } else {
            self.major > major
        }
    }
}

/// iOS version classifications important to Prebid Server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionClassification {
    Unknown,
    Version140,
    Version141,
    Version142OrGreater,
}

/// Detect the iOS version classification from a version string.
pub fn detect_version_classification(v: &str) -> VersionClassification {
    if let Ok(ios_version) = Version::parse(v) {
        if ios_version.equal(14, 0) {
            return VersionClassification::Version140;
        }
        if ios_version.equal(14, 1) {
            return VersionClassification::Version141;
        }
        if ios_version.equal_or_greater(14, 2) {
            return VersionClassification::Version142OrGreater;
        }
    }
    VersionClassification::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        let v = Version::parse("14.2").unwrap();
        assert_eq!(v.major, 14);
        assert_eq!(v.minor, 2);
    }

    #[test]
    fn test_parse_version_with_patch() {
        let v = Version::parse("14.2.1").unwrap();
        assert_eq!(v.major, 14);
        assert_eq!(v.minor, 2);
    }

    #[test]
    fn test_detect_classification() {
        assert_eq!(detect_version_classification("14.0"), VersionClassification::Version140);
        assert_eq!(detect_version_classification("14.1"), VersionClassification::Version141);
        assert_eq!(detect_version_classification("14.2"), VersionClassification::Version142OrGreater);
        assert_eq!(detect_version_classification("15.0"), VersionClassification::Version142OrGreater);
        assert_eq!(detect_version_classification("13.0"), VersionClassification::Unknown);
    }
}
