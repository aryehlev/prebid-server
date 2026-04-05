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
