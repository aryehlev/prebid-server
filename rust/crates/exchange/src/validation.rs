/// OpenRTB request validation and sanitization.

#[derive(Debug, Clone)]
pub enum ValidationError {
    MissingField(String),
    InvalidValue { field: String, reason: String },
    TooManyImps(usize),
    NoImps,
    BothSiteAndApp,
    InvalidImp { index: usize, reason: String },
    NoBidders,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::MissingField(field) => write!(f, "request missing required field: {}", field),
            Self::InvalidValue { field, reason } => {
                write!(f, "invalid value for {}: {}", field, reason)
            }
            Self::TooManyImps(n) => write!(f, "request.imp is too large: {} > 500", n),
            Self::NoImps => write!(f, "request.imp must contain at least one element"),
            Self::BothSiteAndApp => {
                write!(f, "request.site and request.app are mutually exclusive")
            }
            Self::InvalidImp { index, reason } => {
                write!(f, "request.imp[{}]: {}", index, reason)
            }
            Self::NoBidders => {
                write!(f, "request.imp[x].ext must have at least one bidder")
            }
        }
    }
}

/// Validate an OpenRTB BidRequest, returning a list of validation errors.
///
/// Fatal errors (NoImps, BothSiteAndApp) should cause the request to be
/// rejected. Non-fatal errors are warnings that can be logged but allowed.
pub fn validate_request(req: &openrtb::BidRequest) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Must have at least one imp
    if req.imp.is_empty() {
        errors.push(ValidationError::NoImps);
        return errors; // Can't validate further without imps
    }

    // Max imps
    if req.imp.len() > 500 {
        errors.push(ValidationError::TooManyImps(req.imp.len()));
    }

    // Site and App mutually exclusive
    if req.site.is_some() && req.app.is_some() {
        errors.push(ValidationError::BothSiteAndApp);
    }

    // Validate tmax
    if let Some(tmax) = req.tmax {
        if tmax < 0 {
            errors.push(ValidationError::InvalidValue {
                field: "request.tmax".to_string(),
                reason: format!("must be non-negative, got {}", tmax),
            });
        }
    }

    // Validate each imp
    for (i, imp) in req.imp.iter().enumerate() {
        if imp.id.is_empty() {
            errors.push(ValidationError::InvalidImp {
                index: i,
                reason: "imp.id is required".to_string(),
            });
        }
        // Check at least one media type
        if imp.banner.is_none()
            && imp.video.is_none()
            && imp.native.is_none()
            && imp.audio.is_none()
        {
            errors.push(ValidationError::InvalidImp {
                index: i,
                reason: "imp must have at least one of banner/video/native/audio".to_string(),
            });
        }
        // Check bidfloor >= 0
        if imp.bidfloor.map(|f| f < 0.0).unwrap_or(false) {
            errors.push(ValidationError::InvalidImp {
                index: i,
                reason: format!(
                    "bidfloor must be non-negative, got {}",
                    imp.bidfloor.unwrap()
                ),
            });
        }
    }

    errors
}

/// Returns true if any of the given errors are fatal (should reject the request).
pub fn has_fatal_errors(errors: &[ValidationError]) -> bool {
    errors.iter().any(|e| {
        matches!(
            e,
            ValidationError::NoImps | ValidationError::BothSiteAndApp
        )
    })
}

/// Sanitize a BidRequest: remove server-side-only fields that bidders shouldn't see.
pub fn sanitize_request(req: &mut openrtb::BidRequest) {
    if let Some(ext) = &mut req.ext {
        if let Some(obj) = ext.as_object_mut() {
            if let Some(prebid) = obj.get_mut("prebid") {
                if let Some(prebid_obj) = prebid.as_object_mut() {
                    // Remove fields bidders don't need
                    prebid_obj.remove("storedauctionresponse");
                    prebid_obj.remove("storedbidresponse");
                    prebid_obj.remove("server");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Banner validation
// ---------------------------------------------------------------------------

/// Validate banner object on an impression.
pub fn validate_banner(banner: &openrtb::Banner, imp_index: usize) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Must have at least one format OR explicit w+h
    let has_formats = banner.format.as_ref().map(|f| !f.is_empty()).unwrap_or(false);
    let has_wh = banner.w.is_some() && banner.h.is_some();
    if !has_formats && !has_wh {
        errors.push(ValidationError::InvalidImp {
            index: imp_index,
            reason: "banner must have at least one format element or w/h".to_string(),
        });
    }

    // Validate format dimensions are positive
    if let Some(formats) = &banner.format {
        for (fi, fmt) in formats.iter().enumerate() {
            if let Some(w) = fmt.w {
                if w <= 0 {
                    errors.push(ValidationError::InvalidImp {
                        index: imp_index,
                        reason: format!("banner.format[{}].w must be positive, got {}", fi, w),
                    });
                }
            }
            if let Some(h) = fmt.h {
                if h <= 0 {
                    errors.push(ValidationError::InvalidImp {
                        index: imp_index,
                        reason: format!("banner.format[{}].h must be positive, got {}", fi, h),
                    });
                }
            }
        }
    }

    // Validate explicit w/h are positive
    if let Some(w) = banner.w {
        if w <= 0 {
            errors.push(ValidationError::InvalidImp {
                index: imp_index,
                reason: format!("banner.w must be positive, got {}", w),
            });
        }
    }
    if let Some(h) = banner.h {
        if h <= 0 {
            errors.push(ValidationError::InvalidImp {
                index: imp_index,
                reason: format!("banner.h must be positive, got {}", h),
            });
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Video validation
// ---------------------------------------------------------------------------

/// Validate video object on an impression.
pub fn validate_video(video: &openrtb::Video, imp_index: usize) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // mimes should be non-empty
    let has_mimes = video.mimes.as_ref().map(|m| !m.is_empty()).unwrap_or(false);
    if !has_mimes {
        errors.push(ValidationError::InvalidImp {
            index: imp_index,
            reason: "video.mimes must contain at least one MIME type".to_string(),
        });
    }

    // protocols or w+h should be present
    let has_protocols = video.protocols.as_ref().map(|p| !p.is_empty()).unwrap_or(false);
    let has_wh = video.w.is_some() && video.h.is_some();
    if !has_protocols && !has_wh {
        errors.push(ValidationError::InvalidImp {
            index: imp_index,
            reason: "video must have protocols or w/h".to_string(),
        });
    }

    // minduration <= maxduration if both present
    if let (Some(min_d), Some(max_d)) = (video.minduration, video.maxduration) {
        if min_d > max_d {
            errors.push(ValidationError::InvalidImp {
                index: imp_index,
                reason: format!("video.minduration ({}) > video.maxduration ({})", min_d, max_d),
            });
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Audio validation
// ---------------------------------------------------------------------------

/// Validate audio object on an impression.
pub fn validate_audio(audio: &openrtb::Audio, imp_index: usize) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    let has_mimes = audio.mimes.as_ref().map(|m| !m.is_empty()).unwrap_or(false);
    if !has_mimes {
        errors.push(ValidationError::InvalidImp {
            index: imp_index,
            reason: "audio.mimes must contain at least one MIME type".to_string(),
        });
    }

    if let (Some(min_d), Some(max_d)) = (audio.minduration, audio.maxduration) {
        if min_d > max_d {
            errors.push(ValidationError::InvalidImp {
                index: imp_index,
                reason: format!("audio.minduration ({}) > audio.maxduration ({})", min_d, max_d),
            });
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Native validation
// ---------------------------------------------------------------------------

/// Validate native object on an impression.
pub fn validate_native(native: &openrtb::Native, imp_index: usize) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // request field must be present and non-empty
    let request = native.request.as_deref().unwrap_or("");
    if request.is_empty() {
        errors.push(ValidationError::InvalidImp {
            index: imp_index,
            reason: "native.request must be a non-empty JSON string".to_string(),
        });
    } else {
        // Validate it's valid JSON
        if serde_json::from_str::<serde_json::Value>(request).is_err() {
            errors.push(ValidationError::InvalidImp {
                index: imp_index,
                reason: "native.request must be valid JSON".to_string(),
            });
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Bidder ext validation
// ---------------------------------------------------------------------------

/// Extract and validate bidder extensions from imp.ext.
/// Returns a map of bidder_name -> bidder_params, or errors.
pub fn validate_bidder_ext(
    imp: &openrtb::Imp,
    imp_index: usize,
) -> Result<std::collections::HashMap<String, serde_json::Value>, Vec<ValidationError>> {
    let ext = match &imp.ext {
        Some(ext) => ext,
        None => {
            return Err(vec![ValidationError::InvalidImp {
                index: imp_index,
                reason: "imp.ext is required".to_string(),
            }]);
        }
    };

    let obj = match ext.as_object() {
        Some(o) => o,
        None => {
            return Err(vec![ValidationError::InvalidImp {
                index: imp_index,
                reason: "imp.ext must be a JSON object".to_string(),
            }]);
        }
    };

    // Look for imp.ext.prebid.bidder or imp.ext.bidder (legacy)
    let bidder_map = if let Some(prebid) = obj.get("prebid").and_then(|p| p.as_object()) {
        if let Some(bidder) = prebid.get("bidder").and_then(|b| b.as_object()) {
            bidder.clone()
        } else {
            // No prebid.bidder, check for top-level bidder key
            extract_top_level_bidders(obj)
        }
    } else if let Some(bidder) = obj.get("bidder").and_then(|b| b.as_object()) {
        bidder.clone()
    } else {
        extract_top_level_bidders(obj)
    };

    if bidder_map.is_empty() {
        return Err(vec![ValidationError::NoBidders]);
    }

    Ok(bidder_map.into_iter().collect())
}

/// Extract bidder keys from top-level ext (keys that aren't reserved).
fn extract_top_level_bidders(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let reserved = ["prebid", "context", "data", "gpid", "tid", "skadn", "ae"];
    let mut bidders = serde_json::Map::new();
    for (key, value) in obj {
        if !reserved.contains(&key.as_str()) && value.is_object() {
            bidders.insert(key.clone(), value.clone());
        }
    }
    bidders
}

// ---------------------------------------------------------------------------
// Full impression validation (combining all media type validators)
// ---------------------------------------------------------------------------

/// Perform deep validation of all impressions in a request.
pub fn validate_impressions_deep(req: &openrtb::BidRequest) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for (i, imp) in req.imp.iter().enumerate() {
        if let Some(banner) = &imp.banner {
            errors.extend(validate_banner(banner, i));
        }
        if let Some(video) = &imp.video {
            errors.extend(validate_video(video, i));
        }
        if let Some(audio) = &imp.audio {
            errors.extend(validate_audio(audio, i));
        }
        if let Some(native) = &imp.native {
            errors.extend(validate_native(native, i));
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_empty_request() {
        let req = openrtb::BidRequest::default();
        let errors = validate_request(&req);
        assert!(has_fatal_errors(&errors));
    }

    #[test]
    fn test_validate_valid_request() {
        let req = openrtb::BidRequest {
            id: "req1".to_string(),
            imp: vec![openrtb::Imp {
                id: "imp1".to_string(),
                banner: Some(openrtb::Banner::default()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let errors = validate_request(&req);
        assert!(!has_fatal_errors(&errors));
    }

    #[test]
    fn test_validate_site_and_app() {
        let req = openrtb::BidRequest {
            imp: vec![openrtb::Imp {
                id: "imp1".to_string(),
                banner: Some(openrtb::Banner::default()),
                ..Default::default()
            }],
            site: Some(openrtb::Site::default()),
            app: Some(openrtb::App::default()),
            ..Default::default()
        };
        let errors = validate_request(&req);
        assert!(errors.iter().any(|e| matches!(e, ValidationError::BothSiteAndApp)));
    }

    #[test]
    fn test_validate_banner_no_format_no_wh() {
        let banner = openrtb::Banner::default();
        let errors = validate_banner(&banner, 0);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_validate_banner_with_format() {
        let banner = openrtb::Banner {
            format: Some(vec![openrtb::Format { w: Some(300), h: Some(250), ..Default::default() }]),
            ..Default::default()
        };
        let errors = validate_banner(&banner, 0);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_video_no_mimes() {
        let video = openrtb::Video::default();
        let errors = validate_video(&video, 0);
        assert!(errors.iter().any(|e| matches!(e, ValidationError::InvalidImp { reason, .. } if reason.contains("mimes"))));
    }

    #[test]
    fn test_validate_video_valid() {
        let video = openrtb::Video {
            mimes: Some(vec!["video/mp4".to_string()]),
            protocols: Some(vec![1, 2]),
            ..Default::default()
        };
        let errors = validate_video(&video, 0);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_video_duration_mismatch() {
        let video = openrtb::Video {
            mimes: Some(vec!["video/mp4".to_string()]),
            protocols: Some(vec![1]),
            minduration: Some(30),
            maxduration: Some(10),
            ..Default::default()
        };
        let errors = validate_video(&video, 0);
        assert!(errors.iter().any(|e| matches!(e, ValidationError::InvalidImp { reason, .. } if reason.contains("minduration"))));
    }

    #[test]
    fn test_validate_native_empty_request() {
        let native = openrtb::Native::default();
        let errors = validate_native(&native, 0);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_validate_native_valid() {
        let native = openrtb::Native {
            request: Some(r#"{"ver":"1.1","assets":[{"id":1}]}"#.to_string()),
            ..Default::default()
        };
        let errors = validate_native(&native, 0);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_bidder_ext_empty() {
        let imp = openrtb::Imp { id: "imp1".to_string(), ..Default::default() };
        assert!(validate_bidder_ext(&imp, 0).is_err());
    }

    #[test]
    fn test_validate_bidder_ext_with_bidders() {
        let imp = openrtb::Imp {
            id: "imp1".to_string(),
            ext: Some(serde_json::json!({"bidder": {"appnexus": {"placement_id": 123}}})),
            ..Default::default()
        };
        let result = validate_bidder_ext(&imp, 0);
        assert!(result.is_ok());
        assert!(result.unwrap().contains_key("appnexus"));
    }

    #[test]
    fn test_validate_bidder_ext_prebid_format() {
        let imp = openrtb::Imp {
            id: "imp1".to_string(),
            ext: Some(serde_json::json!({"prebid": {"bidder": {"rubicon": {"accountId": 1}}}})),
            ..Default::default()
        };
        let result = validate_bidder_ext(&imp, 0);
        assert!(result.is_ok());
        assert!(result.unwrap().contains_key("rubicon"));
    }

    #[test]
    fn test_sanitize_request_removes_stored() {
        let mut req = openrtb::BidRequest {
            ext: Some(serde_json::json!({"prebid": {"storedauctionresponse": {"id": "123"}, "targeting": {}}})),
            ..Default::default()
        };
        sanitize_request(&mut req);
        let prebid = req.ext.unwrap().get("prebid").cloned().unwrap();
        assert!(prebid.get("storedauctionresponse").is_none());
        assert!(prebid.get("targeting").is_some());
    }
}
