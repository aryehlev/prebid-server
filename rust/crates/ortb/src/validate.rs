//! Request validation for OpenRTB bid requests.
//!
//! Mirrors Go `ortb/request_validator*.go` — validates impressions
//! including banner, video, audio, native, and PMP.

use openrtb::*;

// ---------------------------------------------------------------------------
// Banner validation — mirrors Go ortb/request_validator_banner.go
// ---------------------------------------------------------------------------

/// Validate a banner object on an impression.
/// Mirrors Go `validateBanner`.
pub fn validate_banner(banner: &Option<Banner>, imp_index: usize, is_interstitial: bool) -> Result<(), String> {
    let banner = match banner {
        Some(b) => b,
        None => return Ok(()),
    };

    if let Some(w) = banner.w {
        if w < 0 {
            return Err(format!("request.imp[{}].banner.w must be a positive number", imp_index));
        }
    }
    if let Some(h) = banner.h {
        if h < 0 {
            return Err(format!("request.imp[{}].banner.h must be a positive number", imp_index));
        }
    }

    // Deprecated properties
    if banner.wmin.unwrap_or(0) != 0 {
        return Err(format!("request.imp[{}].banner uses unsupported property: \"wmin\". Use the \"format\" array instead.", imp_index));
    }
    if banner.wmax.unwrap_or(0) != 0 {
        return Err(format!("request.imp[{}].banner uses unsupported property: \"wmax\". Use the \"format\" array instead.", imp_index));
    }
    if banner.hmin.unwrap_or(0) != 0 {
        return Err(format!("request.imp[{}].banner uses unsupported property: \"hmin\". Use the \"format\" array instead.", imp_index));
    }
    if banner.hmax.unwrap_or(0) != 0 {
        return Err(format!("request.imp[{}].banner uses unsupported property: \"hmax\". Use the \"format\" array instead.", imp_index));
    }

    let has_root_size = banner.w.map_or(false, |w| w > 0) && banner.h.map_or(false, |h| h > 0);
    let has_formats = banner.format.as_ref().map_or(false, |f| !f.is_empty());
    if !has_root_size && !has_formats && !is_interstitial {
        return Err(format!(
            "request.imp[{}].banner has no sizes. Define \"w\" and \"h\", or include \"format\" elements.",
            imp_index
        ));
    }

    if let Some(formats) = &banner.format {
        for (i, format) in formats.iter().enumerate() {
            validate_format(format, imp_index, i)?;
        }
    }

    Ok(())
}

/// Validate a banner format entry.
/// Mirrors Go `validateFormat`.
fn validate_format(format: &Format, imp_index: usize, format_index: usize) -> Result<(), String> {
    let w = format.w.unwrap_or(0);
    let h = format.h.unwrap_or(0);
    let wmin = format.wmin.unwrap_or(0);
    let wratio = format.wratio.unwrap_or(0);
    let hratio = format.hratio.unwrap_or(0);

    let uses_hw = w != 0 || h != 0;
    let uses_ratios = wmin != 0 || wratio != 0 || hratio != 0;

    if w < 0 {
        return Err(format!("request.imp[{}].banner.format[{}].w must be a positive number", imp_index, format_index));
    }
    if h < 0 {
        return Err(format!("request.imp[{}].banner.format[{}].h must be a positive number", imp_index, format_index));
    }
    if wratio < 0 {
        return Err(format!("request.imp[{}].banner.format[{}].wratio must be a positive number", imp_index, format_index));
    }
    if hratio < 0 {
        return Err(format!("request.imp[{}].banner.format[{}].hratio must be a positive number", imp_index, format_index));
    }
    if wmin < 0 {
        return Err(format!("request.imp[{}].banner.format[{}].wmin must be a positive number", imp_index, format_index));
    }

    if uses_hw && uses_ratios {
        return Err(format!(
            "Request imp[{}].banner.format[{}] should define *either* {{w, h}} *or* {{wmin, wratio, hratio}}, but not both. If both are valid, send two \"format\" objects in the request.",
            imp_index, format_index
        ));
    }
    if !uses_hw && !uses_ratios {
        return Err(format!(
            "Request imp[{}].banner.format[{}] should define *either* {{w, h}} (for static size requirements) *or* {{wmin, wratio, hratio}} (for flexible sizes) to be non-zero.",
            imp_index, format_index
        ));
    }
    if uses_hw && (w == 0 || h == 0) {
        return Err(format!(
            "Request imp[{}].banner.format[{}] must define non-zero \"h\" and \"w\" properties.",
            imp_index, format_index
        ));
    }
    if uses_ratios && (wmin == 0 || wratio == 0 || hratio == 0) {
        return Err(format!(
            "Request imp[{}].banner.format[{}] must define non-zero \"wmin\", \"wratio\", and \"hratio\" properties.",
            imp_index, format_index
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Video validation — mirrors Go ortb/request_validator_video.go
// ---------------------------------------------------------------------------

/// Validate a video object on an impression.
/// Mirrors Go `validateVideo`.
pub fn validate_video(video: &Option<Video>, imp_index: usize) -> Result<(), String> {
    let video = match video {
        Some(v) => v,
        None => return Ok(()),
    };

    if video.mimes.as_ref().map_or(true, |m| m.is_empty()) {
        return Err(format!(
            "request.imp[{}].video.mimes must contain at least one supported MIME type",
            imp_index
        ));
    }

    if let Some(w) = video.w {
        if w < 0 {
            return Err(format!("request.imp[{}].video.w must be a positive number", imp_index));
        }
    }
    if let Some(h) = video.h {
        if h < 0 {
            return Err(format!("request.imp[{}].video.h must be a positive number", imp_index));
        }
    }
    if video.minbitrate.unwrap_or(0) < 0 {
        return Err(format!("request.imp[{}].video.minbitrate must be a positive number", imp_index));
    }
    if video.maxbitrate.unwrap_or(0) < 0 {
        return Err(format!("request.imp[{}].video.maxbitrate must be a positive number", imp_index));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Audio validation — mirrors Go ortb/request_validator_audio.go
// ---------------------------------------------------------------------------

/// Validate an audio object on an impression.
/// Mirrors Go `validateAudio`.
pub fn validate_audio(audio: &Option<Audio>, imp_index: usize) -> Result<(), String> {
    let audio = match audio {
        Some(a) => a,
        None => return Ok(()),
    };

    if audio.mimes.as_ref().map_or(true, |m| m.is_empty()) {
        return Err(format!(
            "request.imp[{}].audio.mimes must contain at least one supported MIME type",
            imp_index
        ));
    }

    if audio.sequence.unwrap_or(0) < 0 {
        return Err(format!("request.imp[{}].audio.sequence must be a positive number", imp_index));
    }
    if audio.maxseq.unwrap_or(0) < 0 {
        return Err(format!("request.imp[{}].audio.maxseq must be a positive number", imp_index));
    }
    if audio.minbitrate.unwrap_or(0) < 0 {
        return Err(format!("request.imp[{}].audio.minbitrate must be a positive number", imp_index));
    }
    if audio.maxbitrate.unwrap_or(0) < 0 {
        return Err(format!("request.imp[{}].audio.maxbitrate must be a positive number", imp_index));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// PMP validation — mirrors Go ortb/request_validator_pmp.go
// ---------------------------------------------------------------------------

/// Validate a PMP (Private Marketplace) object on an impression.
/// Mirrors Go `validatePmp`.
pub fn validate_pmp(pmp: &Option<Pmp>, imp_index: usize) -> Result<(), String> {
    let pmp = match pmp {
        Some(p) => p,
        None => return Ok(()),
    };

    for (deal_index, deal) in pmp.deals.iter().enumerate() {
        if deal.id.is_empty() {
            return Err(format!(
                "request.imp[{}].pmp.deals[{}] missing required field: \"id\"",
                imp_index, deal_index
            ));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Impression validation — mirrors Go ortb/request_validator.go
// ---------------------------------------------------------------------------

/// Validate a single impression.
/// Mirrors Go `standardRequestValidator.ValidateImp` (without ext/bidder validation).
pub fn validate_imp(imp: &Imp, index: usize) -> Vec<String> {
    if imp.id.is_empty() {
        return vec![format!("request.imp[{}] missing required field: \"id\"", index)];
    }

    if imp.banner.is_none() && imp.video.is_none() && imp.audio.is_none() && imp.native.is_none() {
        return vec![format!(
            "request.imp[{}] must contain at least one of \"banner\", \"video\", \"audio\", or \"native\"",
            index
        )];
    }

    let is_interstitial = imp.instl == Some(1);

    if let Err(e) = validate_banner(&imp.banner, index, is_interstitial) {
        return vec![e];
    }
    if let Err(e) = validate_video(&imp.video, index) {
        return vec![e];
    }
    if let Err(e) = validate_audio(&imp.audio, index) {
        return vec![e];
    }
    if let Err(e) = validate_pmp(&imp.pmp, index) {
        return vec![e];
    }

    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Banner tests --

    #[test]
    fn test_validate_banner_none() {
        assert!(validate_banner(&None, 0, false).is_ok());
    }

    #[test]
    fn test_validate_banner_negative_width() {
        let banner = Some(Banner { w: Some(-1), ..Default::default() });
        assert!(validate_banner(&banner, 0, false).is_err());
    }

    #[test]
    fn test_validate_banner_no_sizes() {
        let banner = Some(Banner::default());
        let result = validate_banner(&banner, 0, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no sizes"));
    }

    #[test]
    fn test_validate_banner_interstitial_no_sizes_ok() {
        let banner = Some(Banner::default());
        assert!(validate_banner(&banner, 0, true).is_ok());
    }

    #[test]
    fn test_validate_banner_with_formats() {
        let banner = Some(Banner {
            format: Some(vec![Format {
                w: Some(300),
                h: Some(250),
                ..Default::default()
            }]),
            ..Default::default()
        });
        assert!(validate_banner(&banner, 0, false).is_ok());
    }

    #[test]
    fn test_validate_banner_with_root_size() {
        let banner = Some(Banner {
            w: Some(300),
            h: Some(250),
            ..Default::default()
        });
        assert!(validate_banner(&banner, 0, false).is_ok());
    }

    // -- Video tests --

    #[test]
    fn test_validate_video_none() {
        assert!(validate_video(&None, 0).is_ok());
    }

    #[test]
    fn test_validate_video_no_mimes() {
        let video = Some(Video::default());
        let result = validate_video(&video, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("mimes"));
    }

    #[test]
    fn test_validate_video_with_mimes() {
        let video = Some(Video {
            mimes: Some(vec!["video/mp4".to_string()]),
            ..Default::default()
        });
        assert!(validate_video(&video, 0).is_ok());
    }

    // -- Audio tests --

    #[test]
    fn test_validate_audio_none() {
        assert!(validate_audio(&None, 0).is_ok());
    }

    #[test]
    fn test_validate_audio_no_mimes() {
        let audio = Some(Audio { mimes: Some(vec![]), ..Default::default() });
        assert!(validate_audio(&audio, 0).is_err());
    }

    // -- PMP tests --

    #[test]
    fn test_validate_pmp_none() {
        assert!(validate_pmp(&None, 0).is_ok());
    }

    #[test]
    fn test_validate_pmp_deal_no_id() {
        let pmp = Some(Pmp {
            deals: vec![Deal { id: "".to_string(), ..Default::default() }],
            ..Default::default()
        });
        assert!(validate_pmp(&pmp, 0).is_err());
    }

    #[test]
    fn test_validate_pmp_deal_with_id() {
        let pmp = Some(Pmp {
            deals: vec![Deal { id: "deal1".to_string(), ..Default::default() }],
            ..Default::default()
        });
        assert!(validate_pmp(&pmp, 0).is_ok());
    }

    // -- Impression tests --

    #[test]
    fn test_validate_imp_no_id() {
        let imp = Imp::default();
        let errors = validate_imp(&imp, 0);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("missing required field"));
    }

    #[test]
    fn test_validate_imp_no_media() {
        let imp = Imp { id: "imp1".to_string(), ..Default::default() };
        let errors = validate_imp(&imp, 0);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("must contain at least one"));
    }

    #[test]
    fn test_validate_imp_valid_banner() {
        let imp = Imp {
            id: "imp1".to_string(),
            banner: Some(Banner {
                w: Some(300),
                h: Some(250),
                ..Default::default()
            }),
            ..Default::default()
        };
        let errors = validate_imp(&imp, 0);
        assert!(errors.is_empty());
    }
}
