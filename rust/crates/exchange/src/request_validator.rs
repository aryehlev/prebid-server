//! OpenRTB request validation ported from the Go `ortb/request_validator*.go` files.
//!
//! This module provides a [`RequestValidator`] trait and a [`StandardRequestValidator`]
//! implementation that validates individual impressions according to the OpenRTB spec,
//! including banner, video, audio, PMP, and native sub-object validation.

use openrtb::{Audio, Banner, Format, Imp, Pmp, Video};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Controls which parts of validation are skipped.
#[derive(Debug, Clone, Default)]
pub struct ValidationConfig {
    /// If true, skip bidder parameter validation in imp.ext.
    pub skip_bidder_params: bool,
    /// If true, skip native validation.
    pub skip_native: bool,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Validates a single impression. Returns a list of error messages (empty = valid).
pub trait RequestValidator {
    fn validate_imp(&self, imp: &Imp, cfg: &ValidationConfig, index: usize) -> Vec<String>;
}

// ---------------------------------------------------------------------------
// StandardRequestValidator
// ---------------------------------------------------------------------------

/// The default implementation of [`RequestValidator`].
#[derive(Debug, Clone, Default)]
pub struct StandardRequestValidator;

impl StandardRequestValidator {
    pub fn new() -> Self {
        Self
    }
}

impl RequestValidator for StandardRequestValidator {
    fn validate_imp(&self, imp: &Imp, cfg: &ValidationConfig, index: usize) -> Vec<String> {
        if imp.id.is_empty() {
            return vec![format!(
                "request.imp[{}] missing required field: \"id\"",
                index
            )];
        }

        if let Some(ref metrics) = imp.metric {
            if !metrics.is_empty() {
                return vec![format!(
                    "request.imp[{}].metric is not yet supported by prebid-server. \
                     Support may be added in the future",
                    index
                )];
            }
        }

        if imp.banner.is_none()
            && imp.video.is_none()
            && imp.audio.is_none()
            && imp.native.is_none()
        {
            return vec![format!(
                "request.imp[{}] must contain at least one of \"banner\", \"video\", \"audio\", or \"native\"",
                index
            )];
        }

        let is_interstitial = imp.instl == Some(1);

        if let Some(ref banner) = imp.banner {
            if let Err(e) = validate_banner(banner, index, is_interstitial) {
                return vec![e];
            }
        }

        if let Some(ref video) = imp.video {
            if let Err(e) = validate_video(video, index) {
                return vec![e];
            }
        }

        if let Some(ref audio) = imp.audio {
            if let Err(e) = validate_audio(audio, index) {
                return vec![e];
            }
        }

        if !cfg.skip_native {
            if let Some(ref _native) = imp.native {
                if let Err(e) = fill_and_validate_native(index) {
                    return vec![e];
                }
            }
        }

        if let Some(ref pmp) = imp.pmp {
            if let Err(e) = validate_pmp(pmp, index) {
                return vec![e];
            }
        }

        // Placeholder: validateImpExt requires ImpWrapper which doesn't exist yet.
        let ext_errors = validate_imp_ext_placeholder(imp, index);
        if !ext_errors.is_empty() {
            return ext_errors;
        }

        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Banner validation
// ---------------------------------------------------------------------------

fn validate_banner(banner: &Banner, imp_index: usize, is_interstitial: bool) -> Result<(), String> {
    // w and h must be non-negative when present.
    if let Some(w) = banner.w {
        if w < 0 {
            return Err(format!(
                "request.imp[{}].banner.w must be a positive number",
                imp_index
            ));
        }
    }
    if let Some(h) = banner.h {
        if h < 0 {
            return Err(format!(
                "request.imp[{}].banner.h must be a positive number",
                imp_index
            ));
        }
    }

    // Deprecated wmin/wmax/hmin/hmax – reject if set.
    if banner.wmin.unwrap_or(0) != 0 {
        return Err(format!(
            "request.imp[{}].banner uses unsupported property: \"wmin\". \
             Use the \"format\" array instead.",
            imp_index
        ));
    }
    if banner.wmax.unwrap_or(0) != 0 {
        return Err(format!(
            "request.imp[{}].banner uses unsupported property: \"wmax\". \
             Use the \"format\" array instead.",
            imp_index
        ));
    }
    if banner.hmin.unwrap_or(0) != 0 {
        return Err(format!(
            "request.imp[{}].banner uses unsupported property: \"hmin\". \
             Use the \"format\" array instead.",
            imp_index
        ));
    }
    if banner.hmax.unwrap_or(0) != 0 {
        return Err(format!(
            "request.imp[{}].banner uses unsupported property: \"hmax\". \
             Use the \"format\" array instead.",
            imp_index
        ));
    }

    let has_root_size = matches!((banner.w, banner.h), (Some(w), Some(h)) if w > 0 && h > 0);
    let formats = banner.format.as_deref().unwrap_or(&[]);
    if !has_root_size && formats.is_empty() && !is_interstitial {
        return Err(format!(
            "request.imp[{}].banner has no sizes. Define \"w\" and \"h\", \
             or include \"format\" elements.",
            imp_index
        ));
    }

    for (i, fmt) in formats.iter().enumerate() {
        validate_format(fmt, imp_index, i)?;
    }

    Ok(())
}

fn validate_format(format: &Format, imp_index: usize, format_index: usize) -> Result<(), String> {
    let w = format.w.unwrap_or(0);
    let h = format.h.unwrap_or(0);
    let wratio = format.wratio.unwrap_or(0);
    let hratio = format.hratio.unwrap_or(0);
    let wmin = format.wmin.unwrap_or(0);

    let uses_hw = w != 0 || h != 0;
    let uses_ratios = wmin != 0 || wratio != 0 || hratio != 0;

    // Non-negative checks
    if w < 0 {
        return Err(format!(
            "request.imp[{}].banner.format[{}].w must be a positive number",
            imp_index, format_index
        ));
    }
    if h < 0 {
        return Err(format!(
            "request.imp[{}].banner.format[{}].h must be a positive number",
            imp_index, format_index
        ));
    }
    if wratio < 0 {
        return Err(format!(
            "request.imp[{}].banner.format[{}].wratio must be a positive number",
            imp_index, format_index
        ));
    }
    if hratio < 0 {
        return Err(format!(
            "request.imp[{}].banner.format[{}].hratio must be a positive number",
            imp_index, format_index
        ));
    }
    if wmin < 0 {
        return Err(format!(
            "request.imp[{}].banner.format[{}].wmin must be a positive number",
            imp_index, format_index
        ));
    }

    if uses_hw && uses_ratios {
        return Err(format!(
            "Request imp[{}].banner.format[{}] should define *either* {{w, h}} *or* \
             {{wmin, wratio, hratio}}, but not both. If both are valid, send two \
             \"format\" objects in the request.",
            imp_index, format_index
        ));
    }
    if !uses_hw && !uses_ratios {
        return Err(format!(
            "Request imp[{}].banner.format[{}] should define *either* {{w, h}} \
             (for static size requirements) *or* {{wmin, wratio, hratio}} \
             (for flexible sizes) to be non-zero.",
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
            "Request imp[{}].banner.format[{}] must define non-zero \"wmin\", \"wratio\", \
             and \"hratio\" properties.",
            imp_index, format_index
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Video validation
// ---------------------------------------------------------------------------

fn validate_video(video: &Video, imp_index: usize) -> Result<(), String> {
    let has_mimes = video
        .mimes
        .as_ref()
        .map(|m| !m.is_empty())
        .unwrap_or(false);
    if !has_mimes {
        return Err(format!(
            "request.imp[{}].video.mimes must contain at least one supported MIME type",
            imp_index
        ));
    }

    if let Some(w) = video.w {
        if w < 0 {
            return Err(format!(
                "request.imp[{}].video.w must be a positive number",
                imp_index
            ));
        }
    }
    if let Some(h) = video.h {
        if h < 0 {
            return Err(format!(
                "request.imp[{}].video.h must be a positive number",
                imp_index
            ));
        }
    }
    if let Some(minbitrate) = video.minbitrate {
        if minbitrate < 0 {
            return Err(format!(
                "request.imp[{}].video.minbitrate must be a positive number",
                imp_index
            ));
        }
    }
    if let Some(maxbitrate) = video.maxbitrate {
        if maxbitrate < 0 {
            return Err(format!(
                "request.imp[{}].video.maxbitrate must be a positive number",
                imp_index
            ));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Audio validation
// ---------------------------------------------------------------------------

fn validate_audio(audio: &Audio, imp_index: usize) -> Result<(), String> {
    let has_mimes = audio
        .mimes
        .as_ref()
        .map(|m| !m.is_empty())
        .unwrap_or(false);
    if !has_mimes {
        return Err(format!(
            "request.imp[{}].audio.mimes must contain at least one supported MIME type",
            imp_index
        ));
    }

    if let Some(seq) = audio.sequence {
        if seq < 0 {
            return Err(format!(
                "request.imp[{}].audio.sequence must be a positive number",
                imp_index
            ));
        }
    }
    if let Some(maxseq) = audio.maxseq {
        if maxseq < 0 {
            return Err(format!(
                "request.imp[{}].audio.maxseq must be a positive number",
                imp_index
            ));
        }
    }
    if let Some(minbitrate) = audio.minbitrate {
        if minbitrate < 0 {
            return Err(format!(
                "request.imp[{}].audio.minbitrate must be a positive number",
                imp_index
            ));
        }
    }
    if let Some(maxbitrate) = audio.maxbitrate {
        if maxbitrate < 0 {
            return Err(format!(
                "request.imp[{}].audio.maxbitrate must be a positive number",
                imp_index
            ));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// PMP validation
// ---------------------------------------------------------------------------

fn validate_pmp(pmp: &Pmp, imp_index: usize) -> Result<(), String> {
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
// Native validation (placeholder)
// ---------------------------------------------------------------------------

/// Placeholder for native validation. The full implementation requires native
/// request types that are not yet available in the Rust openrtb crate.
fn fill_and_validate_native(_imp_index: usize) -> Result<(), String> {
    Ok(())
}

// ---------------------------------------------------------------------------
// Imp ext validation (placeholder)
// ---------------------------------------------------------------------------

/// Placeholder for imp.ext validation. The full implementation requires
/// `ImpWrapper` which does not exist in the Rust codebase yet.
fn validate_imp_ext_placeholder(_imp: &Imp, _imp_index: usize) -> Vec<String> {
    Vec::new()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_cfg() -> ValidationConfig {
        ValidationConfig::default()
    }

    // -- Helpers --

    fn make_imp_with_banner(banner: Banner) -> Imp {
        Imp {
            id: "imp-1".to_string(),
            banner: Some(banner),
            ..Default::default()
        }
    }

    fn make_valid_banner() -> Banner {
        Banner {
            w: Some(300),
            h: Some(250),
            ..Default::default()
        }
    }

    fn make_valid_video() -> Video {
        Video {
            mimes: Some(vec!["video/mp4".to_string()]),
            w: Some(640),
            h: Some(480),
            ..Default::default()
        }
    }

    fn make_valid_audio() -> Audio {
        Audio {
            mimes: Some(vec!["audio/mp3".to_string()]),
            ..Default::default()
        }
    }

    // -----------------------------------------------------------------------
    // ValidateImp – top-level
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_imp_missing_id() {
        let v = StandardRequestValidator::new();
        let imp = Imp {
            id: "".to_string(),
            banner: Some(make_valid_banner()),
            ..Default::default()
        };
        let errs = v.validate_imp(&imp, &default_cfg(), 0);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("missing required field: \"id\""));
    }

    #[test]
    fn test_validate_imp_metric_unsupported() {
        let v = StandardRequestValidator::new();
        let imp = Imp {
            id: "imp1".to_string(),
            banner: Some(make_valid_banner()),
            metric: Some(vec![openrtb::Metric {
                metric_type: "viewability".to_string(),
                value: 0.5,
                ..Default::default()
            }]),
            ..Default::default()
        };
        let errs = v.validate_imp(&imp, &default_cfg(), 0);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("metric is not yet supported"));
    }

    #[test]
    fn test_validate_imp_no_media_type() {
        let v = StandardRequestValidator::new();
        let imp = Imp {
            id: "imp1".to_string(),
            ..Default::default()
        };
        let errs = v.validate_imp(&imp, &default_cfg(), 0);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("must contain at least one of"));
    }

    #[test]
    fn test_validate_imp_valid_banner() {
        let v = StandardRequestValidator::new();
        let imp = make_imp_with_banner(make_valid_banner());
        let errs = v.validate_imp(&imp, &default_cfg(), 0);
        assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
    }

    // -----------------------------------------------------------------------
    // Banner validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_banner_negative_w() {
        let err = validate_banner(
            &Banner {
                w: Some(-1),
                h: Some(250),
                ..Default::default()
            },
            0,
            false,
        );
        assert!(err.unwrap_err().contains("banner.w must be a positive number"));
    }

    #[test]
    fn test_banner_negative_h() {
        let err = validate_banner(
            &Banner {
                w: Some(300),
                h: Some(-1),
                ..Default::default()
            },
            0,
            false,
        );
        assert!(err.unwrap_err().contains("banner.h must be a positive number"));
    }

    #[test]
    fn test_banner_deprecated_wmin() {
        let err = validate_banner(
            &Banner {
                w: Some(300),
                h: Some(250),
                wmin: Some(100),
                ..Default::default()
            },
            0,
            false,
        );
        assert!(err.unwrap_err().contains("unsupported property: \"wmin\""));
    }

    #[test]
    fn test_banner_deprecated_wmax() {
        let err = validate_banner(
            &Banner {
                w: Some(300),
                h: Some(250),
                wmax: Some(1000),
                ..Default::default()
            },
            0,
            false,
        );
        assert!(err.unwrap_err().contains("unsupported property: \"wmax\""));
    }

    #[test]
    fn test_banner_deprecated_hmin() {
        let err = validate_banner(
            &Banner {
                w: Some(300),
                h: Some(250),
                hmin: Some(100),
                ..Default::default()
            },
            0,
            false,
        );
        assert!(err.unwrap_err().contains("unsupported property: \"hmin\""));
    }

    #[test]
    fn test_banner_deprecated_hmax() {
        let err = validate_banner(
            &Banner {
                w: Some(300),
                h: Some(250),
                hmax: Some(1000),
                ..Default::default()
            },
            0,
            false,
        );
        assert!(err.unwrap_err().contains("unsupported property: \"hmax\""));
    }

    #[test]
    fn test_banner_no_sizes_not_interstitial() {
        let err = validate_banner(&Banner::default(), 0, false);
        assert!(err.unwrap_err().contains("has no sizes"));
    }

    #[test]
    fn test_banner_no_sizes_interstitial_ok() {
        let result = validate_banner(&Banner::default(), 0, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_banner_with_format() {
        let result = validate_banner(
            &Banner {
                format: Some(vec![Format {
                    w: Some(300),
                    h: Some(250),
                    ..Default::default()
                }]),
                ..Default::default()
            },
            0,
            false,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_banner_with_wh() {
        let result = validate_banner(
            &Banner {
                w: Some(300),
                h: Some(250),
                ..Default::default()
            },
            0,
            false,
        );
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Format validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_valid_hw() {
        assert!(validate_format(
            &Format {
                w: Some(300),
                h: Some(250),
                ..Default::default()
            },
            0,
            0,
        )
        .is_ok());
    }

    #[test]
    fn test_format_valid_ratios() {
        assert!(validate_format(
            &Format {
                wmin: Some(100),
                wratio: Some(4),
                hratio: Some(3),
                ..Default::default()
            },
            0,
            0,
        )
        .is_ok());
    }

    #[test]
    fn test_format_both_hw_and_ratios() {
        let err = validate_format(
            &Format {
                w: Some(300),
                h: Some(250),
                wmin: Some(100),
                wratio: Some(4),
                hratio: Some(3),
                ..Default::default()
            },
            0,
            0,
        );
        assert!(err.unwrap_err().contains("should define *either*"));
    }

    #[test]
    fn test_format_neither_hw_nor_ratios() {
        let err = validate_format(&Format::default(), 0, 0);
        assert!(err.unwrap_err().contains("should define *either*"));
    }

    #[test]
    fn test_format_hw_w_only() {
        let err = validate_format(
            &Format {
                w: Some(300),
                ..Default::default()
            },
            0,
            0,
        );
        assert!(err
            .unwrap_err()
            .contains("must define non-zero \"h\" and \"w\""));
    }

    #[test]
    fn test_format_ratios_missing_wmin() {
        let err = validate_format(
            &Format {
                wratio: Some(4),
                hratio: Some(3),
                ..Default::default()
            },
            0,
            0,
        );
        assert!(err.unwrap_err().contains("must define non-zero \"wmin\""));
    }

    #[test]
    fn test_format_negative_w() {
        let err = validate_format(
            &Format {
                w: Some(-1),
                h: Some(250),
                ..Default::default()
            },
            0,
            0,
        );
        assert!(err.unwrap_err().contains("format[0].w must be a positive"));
    }

    #[test]
    fn test_format_negative_h() {
        let err = validate_format(
            &Format {
                w: Some(300),
                h: Some(-1),
                ..Default::default()
            },
            0,
            0,
        );
        assert!(err.unwrap_err().contains("format[0].h must be a positive"));
    }

    #[test]
    fn test_format_negative_wratio() {
        let err = validate_format(
            &Format {
                wratio: Some(-1),
                hratio: Some(3),
                wmin: Some(100),
                ..Default::default()
            },
            0,
            0,
        );
        assert!(err
            .unwrap_err()
            .contains("format[0].wratio must be a positive"));
    }

    #[test]
    fn test_format_negative_hratio() {
        let err = validate_format(
            &Format {
                wratio: Some(4),
                hratio: Some(-1),
                wmin: Some(100),
                ..Default::default()
            },
            0,
            0,
        );
        assert!(err
            .unwrap_err()
            .contains("format[0].hratio must be a positive"));
    }

    #[test]
    fn test_format_negative_wmin() {
        let err = validate_format(
            &Format {
                wmin: Some(-1),
                wratio: Some(4),
                hratio: Some(3),
                ..Default::default()
            },
            0,
            0,
        );
        assert!(err
            .unwrap_err()
            .contains("format[0].wmin must be a positive"));
    }

    // -----------------------------------------------------------------------
    // Video validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_video_no_mimes() {
        let err = validate_video(&Video::default(), 0);
        assert!(err.unwrap_err().contains("video.mimes must contain"));
    }

    #[test]
    fn test_video_empty_mimes() {
        let err = validate_video(
            &Video {
                mimes: Some(vec![]),
                ..Default::default()
            },
            0,
        );
        assert!(err.unwrap_err().contains("video.mimes must contain"));
    }

    #[test]
    fn test_video_valid() {
        assert!(validate_video(&make_valid_video(), 0).is_ok());
    }

    #[test]
    fn test_video_negative_w() {
        let err = validate_video(
            &Video {
                mimes: Some(vec!["video/mp4".to_string()]),
                w: Some(-1),
                ..Default::default()
            },
            0,
        );
        assert!(err.unwrap_err().contains("video.w must be a positive"));
    }

    #[test]
    fn test_video_negative_h() {
        let err = validate_video(
            &Video {
                mimes: Some(vec!["video/mp4".to_string()]),
                h: Some(-1),
                ..Default::default()
            },
            0,
        );
        assert!(err.unwrap_err().contains("video.h must be a positive"));
    }

    #[test]
    fn test_video_negative_minbitrate() {
        let err = validate_video(
            &Video {
                mimes: Some(vec!["video/mp4".to_string()]),
                minbitrate: Some(-1),
                ..Default::default()
            },
            0,
        );
        assert!(err
            .unwrap_err()
            .contains("video.minbitrate must be a positive"));
    }

    #[test]
    fn test_video_negative_maxbitrate() {
        let err = validate_video(
            &Video {
                mimes: Some(vec!["video/mp4".to_string()]),
                maxbitrate: Some(-1),
                ..Default::default()
            },
            0,
        );
        assert!(err
            .unwrap_err()
            .contains("video.maxbitrate must be a positive"));
    }

    // -----------------------------------------------------------------------
    // Audio validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_audio_no_mimes() {
        let err = validate_audio(&Audio::default(), 0);
        assert!(err.unwrap_err().contains("audio.mimes must contain"));
    }

    #[test]
    fn test_audio_empty_mimes() {
        let err = validate_audio(
            &Audio {
                mimes: Some(vec![]),
                ..Default::default()
            },
            0,
        );
        assert!(err.unwrap_err().contains("audio.mimes must contain"));
    }

    #[test]
    fn test_audio_valid() {
        assert!(validate_audio(&make_valid_audio(), 0).is_ok());
    }

    #[test]
    fn test_audio_negative_sequence() {
        let err = validate_audio(
            &Audio {
                mimes: Some(vec!["audio/mp3".to_string()]),
                sequence: Some(-1),
                ..Default::default()
            },
            0,
        );
        assert!(err
            .unwrap_err()
            .contains("audio.sequence must be a positive"));
    }

    #[test]
    fn test_audio_negative_maxseq() {
        let err = validate_audio(
            &Audio {
                mimes: Some(vec!["audio/mp3".to_string()]),
                maxseq: Some(-1),
                ..Default::default()
            },
            0,
        );
        assert!(err
            .unwrap_err()
            .contains("audio.maxseq must be a positive"));
    }

    #[test]
    fn test_audio_negative_minbitrate() {
        let err = validate_audio(
            &Audio {
                mimes: Some(vec!["audio/mp3".to_string()]),
                minbitrate: Some(-1),
                ..Default::default()
            },
            0,
        );
        assert!(err
            .unwrap_err()
            .contains("audio.minbitrate must be a positive"));
    }

    #[test]
    fn test_audio_negative_maxbitrate() {
        let err = validate_audio(
            &Audio {
                mimes: Some(vec!["audio/mp3".to_string()]),
                maxbitrate: Some(-1),
                ..Default::default()
            },
            0,
        );
        assert!(err
            .unwrap_err()
            .contains("audio.maxbitrate must be a positive"));
    }

    // -----------------------------------------------------------------------
    // PMP validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_pmp_empty_deals() {
        assert!(validate_pmp(
            &Pmp {
                deals: vec![],
                ..Default::default()
            },
            0
        )
        .is_ok());
    }

    #[test]
    fn test_pmp_valid_deal() {
        assert!(validate_pmp(
            &Pmp {
                deals: vec![openrtb::Deal {
                    id: "deal-1".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            0
        )
        .is_ok());
    }

    #[test]
    fn test_pmp_deal_missing_id() {
        let err = validate_pmp(
            &Pmp {
                deals: vec![openrtb::Deal {
                    id: "".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            0,
        );
        assert!(err
            .unwrap_err()
            .contains("pmp.deals[0] missing required field: \"id\""));
    }

    #[test]
    fn test_pmp_second_deal_missing_id() {
        let err = validate_pmp(
            &Pmp {
                deals: vec![
                    openrtb::Deal {
                        id: "deal-1".to_string(),
                        ..Default::default()
                    },
                    openrtb::Deal {
                        id: "".to_string(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            0,
        );
        assert!(err
            .unwrap_err()
            .contains("pmp.deals[1] missing required field: \"id\""));
    }

    // -----------------------------------------------------------------------
    // Native validation (placeholder)
    // -----------------------------------------------------------------------

    #[test]
    fn test_native_placeholder_ok() {
        assert!(fill_and_validate_native(0).is_ok());
    }

    // -----------------------------------------------------------------------
    // Full imp validation integration
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_imp_with_video() {
        let v = StandardRequestValidator::new();
        let imp = Imp {
            id: "imp1".to_string(),
            video: Some(make_valid_video()),
            ..Default::default()
        };
        let errs = v.validate_imp(&imp, &default_cfg(), 0);
        assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
    }

    #[test]
    fn test_validate_imp_with_audio() {
        let v = StandardRequestValidator::new();
        let imp = Imp {
            id: "imp1".to_string(),
            audio: Some(make_valid_audio()),
            ..Default::default()
        };
        let errs = v.validate_imp(&imp, &default_cfg(), 0);
        assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
    }

    #[test]
    fn test_validate_imp_with_pmp_invalid_deal() {
        let v = StandardRequestValidator::new();
        let imp = Imp {
            id: "imp1".to_string(),
            banner: Some(make_valid_banner()),
            pmp: Some(Pmp {
                deals: vec![openrtb::Deal {
                    id: "".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let errs = v.validate_imp(&imp, &default_cfg(), 0);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("pmp.deals[0] missing required field"));
    }

    #[test]
    fn test_validate_imp_interstitial_no_banner_sizes() {
        let v = StandardRequestValidator::new();
        let imp = Imp {
            id: "imp1".to_string(),
            banner: Some(Banner::default()),
            instl: Some(1),
            ..Default::default()
        };
        let errs = v.validate_imp(&imp, &default_cfg(), 0);
        assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
    }

    #[test]
    fn test_validate_imp_skip_native() {
        let v = StandardRequestValidator::new();
        let imp = Imp {
            id: "imp1".to_string(),
            native: Some(openrtb::Native::default()),
            ..Default::default()
        };
        let cfg = ValidationConfig {
            skip_native: true,
            ..Default::default()
        };
        let errs = v.validate_imp(&imp, &cfg, 0);
        assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
    }

    #[test]
    fn test_validation_config_default() {
        let cfg = ValidationConfig::default();
        assert!(!cfg.skip_bidder_params);
        assert!(!cfg.skip_native);
    }
}
