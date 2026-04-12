//! Interstitial ad size processing.
//!
//! Mirrors Go `endpoints/openrtb2/interstitial.go` — resolves interstitial
//! ad sizes based on device dimensions and publisher min-size preferences.

use openrtb::{BidRequest, Device, Format, Imp};

// ---------------------------------------------------------------------------
// InterstitialSize — mirrors Go config.InterstitialSize
// ---------------------------------------------------------------------------

/// Width/height pair for an interstitial ad format.
#[derive(Debug, Clone, Copy)]
pub struct InterstitialSize {
    pub width: u64,
    pub height: u64,
}

/// Resolved list of interstitial sizes, sorted by size (larger first) and frequency.
///
/// Mirrors Go `config.ResolvedInterstitialSizes`. Originally sourced from
/// AppNexus/Xandr stats.
pub const RESOLVED_INTERSTITIAL_SIZES: &[InterstitialSize] = &[
    InterstitialSize { width: 300, height: 250 },
    InterstitialSize { width: 728, height: 90 },
    InterstitialSize { width: 160, height: 600 },
    InterstitialSize { width: 320, height: 50 },
    InterstitialSize { width: 300, height: 600 },
    InterstitialSize { width: 970, height: 250 },
    InterstitialSize { width: 2000, height: 1400 },
    InterstitialSize { width: 1920, height: 1200 },
    InterstitialSize { width: 1800, height: 1000 },
    InterstitialSize { width: 1920, height: 1080 },
    InterstitialSize { width: 1600, height: 1150 },
    InterstitialSize { width: 1696, height: 900 },
    InterstitialSize { width: 1600, height: 900 },
    InterstitialSize { width: 1270, height: 800 },
    InterstitialSize { width: 970, height: 1000 },
    InterstitialSize { width: 1920, height: 480 },
    InterstitialSize { width: 320, height: 320 },
    InterstitialSize { width: 1600, height: 500 },
    InterstitialSize { width: 768, height: 1024 },
    InterstitialSize { width: 1024, height: 768 },
    InterstitialSize { width: 828, height: 910 },
    InterstitialSize { width: 728, height: 970 },
    InterstitialSize { width: 120, height: 600 },
    InterstitialSize { width: 640, height: 960 },
    InterstitialSize { width: 980, height: 600 },
    InterstitialSize { width: 620, height: 891 },
    InterstitialSize { width: 930, height: 600 },
    InterstitialSize { width: 980, height: 552 },
    InterstitialSize { width: 1272, height: 328 },
    InterstitialSize { width: 300, height: 50 },
    InterstitialSize { width: 500, height: 1000 },
    InterstitialSize { width: 900, height: 550 },
    InterstitialSize { width: 980, height: 500 },
    InterstitialSize { width: 970, height: 500 },
    InterstitialSize { width: 800, height: 600 },
    InterstitialSize { width: 336, height: 280 },
    InterstitialSize { width: 1250, height: 360 },
    InterstitialSize { width: 980, height: 400 },
    InterstitialSize { width: 320, height: 250 },
    InterstitialSize { width: 320, height: 480 },
    InterstitialSize { width: 980, height: 240 },
    InterstitialSize { width: 580, height: 400 },
    InterstitialSize { width: 970, height: 415 },
    InterstitialSize { width: 480, height: 820 },
    InterstitialSize { width: 620, height: 620 },
    InterstitialSize { width: 980, height: 300 },
    InterstitialSize { width: 970, height: 90 },
    InterstitialSize { width: 600, height: 600 },
    InterstitialSize { width: 1800, height: 200 },
    InterstitialSize { width: 970, height: 310 },
    InterstitialSize { width: 720, height: 480 },
    InterstitialSize { width: 1295, height: 250 },
    InterstitialSize { width: 300, height: 1050 },
    InterstitialSize { width: 1272, height: 250 },
    InterstitialSize { width: 300, height: 300 },
    InterstitialSize { width: 640, height: 480 },
    InterstitialSize { width: 320, height: 100 },
    InterstitialSize { width: 580, height: 500 },
    InterstitialSize { width: 1000, height: 300 },
    InterstitialSize { width: 1250, height: 240 },
    InterstitialSize { width: 600, height: 500 },
    InterstitialSize { width: 300, height: 1000 },
    InterstitialSize { width: 728, height: 410 },
    InterstitialSize { width: 800, height: 250 },
    InterstitialSize { width: 970, height: 300 },
    InterstitialSize { width: 950, height: 300 },
    InterstitialSize { width: 994, height: 250 },
    InterstitialSize { width: 940, height: 300 },
    InterstitialSize { width: 640, height: 320 },
    InterstitialSize { width: 468, height: 600 },
    InterstitialSize { width: 970, height: 200 },
    InterstitialSize { width: 930, height: 180 },
    InterstitialSize { width: 250, height: 600 },
    InterstitialSize { width: 491, height: 555 },
    InterstitialSize { width: 550, height: 480 },
    InterstitialSize { width: 750, height: 300 },
    InterstitialSize { width: 980, height: 250 },
    InterstitialSize { width: 1000, height: 260 },
    InterstitialSize { width: 980, height: 150 },
    InterstitialSize { width: 350, height: 240 },
    InterstitialSize { width: 970, height: 210 },
    InterstitialSize { width: 640, height: 360 },
    InterstitialSize { width: 580, height: 415 },
    InterstitialSize { width: 480, height: 300 },
    InterstitialSize { width: 750, height: 200 },
    InterstitialSize { width: 360, height: 640 },
    InterstitialSize { width: 624, height: 368 },
    InterstitialSize { width: 900, height: 250 },
    InterstitialSize { width: 468, height: 400 },
    InterstitialSize { width: 608, height: 226 },
    InterstitialSize { width: 690, height: 300 },
    InterstitialSize { width: 605, height: 340 },
    InterstitialSize { width: 320, height: 640 },
    InterstitialSize { width: 450, height: 450 },
    InterstitialSize { width: 300, height: 480 },
    InterstitialSize { width: 250, height: 800 },
    InterstitialSize { width: 640, height: 300 },
    InterstitialSize { width: 320, height: 160 },
    InterstitialSize { width: 980, height: 200 },
    InterstitialSize { width: 950, height: 200 },
    InterstitialSize { width: 480, height: 400 },
    InterstitialSize { width: 740, height: 250 },
    InterstitialSize { width: 336, height: 544 },
    InterstitialSize { width: 303, height: 603 },
    InterstitialSize { width: 320, height: 568 },
    InterstitialSize { width: 301, height: 601 },
    InterstitialSize { width: 300, height: 601 },
    InterstitialSize { width: 600, height: 300 },
    InterstitialSize { width: 180, height: 500 },
    InterstitialSize { width: 980, height: 120 },
    InterstitialSize { width: 950, height: 180 },
    InterstitialSize { width: 935, height: 180 },
    InterstitialSize { width: 994, height: 170 },
    InterstitialSize { width: 468, height: 360 },
    InterstitialSize { width: 320, height: 400 },
    InterstitialSize { width: 320, height: 240 },
    InterstitialSize { width: 320, height: 500 },
    InterstitialSize { width: 316, height: 513 },
    InterstitialSize { width: 630, height: 250 },
    InterstitialSize { width: 480, height: 320 },
    InterstitialSize { width: 320, height: 481 },
    InterstitialSize { width: 520, height: 290 },
    InterstitialSize { width: 250, height: 250 },
    InterstitialSize { width: 300, height: 500 },
    InterstitialSize { width: 1000, height: 150 },
    InterstitialSize { width: 320, height: 460 },
    InterstitialSize { width: 970, height: 150 },
    InterstitialSize { width: 800, height: 180 },
    InterstitialSize { width: 468, height: 60 },
    InterstitialSize { width: 482, height: 282 },
    InterstitialSize { width: 680, height: 200 },
    InterstitialSize { width: 320, height: 416 },
    InterstitialSize { width: 480, height: 280 },
    InterstitialSize { width: 300, height: 431 },
    InterstitialSize { width: 728, height: 180 },
    InterstitialSize { width: 300, height: 430 },
    InterstitialSize { width: 180, height: 701 },
    InterstitialSize { width: 840, height: 150 },
    InterstitialSize { width: 600, height: 200 },
    InterstitialSize { width: 768, height: 150 },
    InterstitialSize { width: 200, height: 600 },
    InterstitialSize { width: 350, height: 350 },
    InterstitialSize { width: 202, height: 600 },
    InterstitialSize { width: 400, height: 300 },
    InterstitialSize { width: 414, height: 286 },
    InterstitialSize { width: 656, height: 180 },
    InterstitialSize { width: 994, height: 118 },
    InterstitialSize { width: 638, height: 180 },
    InterstitialSize { width: 650, height: 170 },
    InterstitialSize { width: 1000, height: 90 },
    InterstitialSize { width: 300, height: 360 },
    InterstitialSize { width: 600, height: 180 },
    InterstitialSize { width: 240, height: 400 },
    InterstitialSize { width: 161, height: 601 },
    InterstitialSize { width: 610, height: 138 },
    InterstitialSize { width: 164, height: 601 },
    InterstitialSize { width: 980, height: 100 },
    InterstitialSize { width: 970, height: 100 },
    InterstitialSize { width: 468, height: 200 },
    InterstitialSize { width: 250, height: 360 },
    InterstitialSize { width: 320, height: 180 },
    InterstitialSize { width: 605, height: 150 },
    InterstitialSize { width: 600, height: 150 },
    InterstitialSize { width: 980, height: 90 },
    InterstitialSize { width: 750, height: 100 },
    InterstitialSize { width: 150, height: 600 },
    InterstitialSize { width: 630, height: 140 },
    InterstitialSize { width: 696, height: 120 },
    InterstitialSize { width: 307, height: 254 },
    InterstitialSize { width: 303, height: 253 },
    InterstitialSize { width: 703, height: 110 },
    InterstitialSize { width: 550, height: 140 },
    InterstitialSize { width: 300, height: 251 },
    InterstitialSize { width: 298, height: 250 },
    InterstitialSize { width: 500, height: 150 },
    InterstitialSize { width: 413, height: 180 },
    InterstitialSize { width: 728, height: 100 },
    InterstitialSize { width: 269, height: 269 },
    InterstitialSize { width: 640, height: 106 },
    InterstitialSize { width: 768, height: 90 },
    InterstitialSize { width: 320, height: 200 },
    InterstitialSize { width: 728, height: 93 },
    InterstitialSize { width: 729, height: 90 },
    InterstitialSize { width: 727, height: 90 },
    InterstitialSize { width: 640, height: 100 },
    InterstitialSize { width: 720, height: 90 },
    InterstitialSize { width: 300, height: 100 },
    InterstitialSize { width: 970, height: 66 },
    InterstitialSize { width: 480, height: 110 },
    InterstitialSize { width: 300, height: 200 },
    InterstitialSize { width: 707, height: 83 },
    InterstitialSize { width: 900, height: 65 },
    InterstitialSize { width: 467, height: 120 },
    InterstitialSize { width: 200, height: 200 },
    InterstitialSize { width: 450, height: 121 },
    InterstitialSize { width: 320, height: 150 },
    InterstitialSize { width: 600, height: 90 },
    InterstitialSize { width: 300, height: 170 },
    InterstitialSize { width: 468, height: 100 },
    InterstitialSize { width: 300, height: 169 },
    InterstitialSize { width: 500, height: 100 },
    InterstitialSize { width: 300, height: 150 },
    InterstitialSize { width: 990, height: 50 },
    InterstitialSize { width: 140, height: 350 },
    InterstitialSize { width: 160, height: 300 },
    InterstitialSize { width: 300, height: 158 },
    InterstitialSize { width: 190, height: 240 },
    InterstitialSize { width: 180, height: 150 },
    InterstitialSize { width: 300, height: 145 },
    InterstitialSize { width: 310, height: 122 },
    InterstitialSize { width: 468, height: 90 },
    InterstitialSize { width: 594, height: 70 },
    InterstitialSize { width: 480, height: 80 },
    InterstitialSize { width: 600, height: 65 },
    InterstitialSize { width: 484, height: 80 },
    InterstitialSize { width: 320, height: 75 },
    InterstitialSize { width: 335, height: 100 },
    InterstitialSize { width: 375, height: 80 },
    InterstitialSize { width: 300, height: 75 },
    InterstitialSize { width: 120, height: 240 },
    InterstitialSize { width: 480, height: 60 },
    InterstitialSize { width: 300, height: 90 },
    InterstitialSize { width: 120, height: 60 },
    InterstitialSize { width: 100, height: 200 },
    InterstitialSize { width: 320, height: 80 },
    InterstitialSize { width: 160, height: 160 },
    InterstitialSize { width: 400, height: 63 },
    InterstitialSize { width: 300, height: 81 },
    InterstitialSize { width: 1, height: 1 },
    InterstitialSize { width: 300, height: 80 },
    InterstitialSize { width: 375, height: 58 },
    InterstitialSize { width: 232, height: 90 },
    InterstitialSize { width: 321, height: 51 },
    InterstitialSize { width: 320, height: 63 },
    InterstitialSize { width: 319, height: 49 },
    InterstitialSize { width: 300, height: 65 },
    InterstitialSize { width: 360, height: 50 },
    InterstitialSize { width: 125, height: 125 },
    InterstitialSize { width: 298, height: 60 },
    InterstitialSize { width: 300, height: 60 },
    InterstitialSize { width: 299, height: 60 },
    InterstitialSize { width: 301, height: 50 },
    InterstitialSize { width: 234, height: 60 },
    InterstitialSize { width: 280, height: 47 },
    InterstitialSize { width: 120, height: 90 },
    InterstitialSize { width: 13, height: 13 },
    InterstitialSize { width: 17, height: 17 },
    InterstitialSize { width: 168, height: 50 },
    InterstitialSize { width: 140, height: 50 },
    InterstitialSize { width: 120, height: 20 },
];

// ---------------------------------------------------------------------------
// InterstitialConfig — publisher min-size preferences from device.ext.prebid
// ---------------------------------------------------------------------------

/// Publisher-specified minimum interstitial size as percentages of max.
/// Mirrors Go `openrtb_ext.ExtDeviceInterstitial`.
#[derive(Debug, Clone, Copy, Default)]
pub struct InterstitialConfig {
    pub min_width_perc: i64,
    pub min_height_perc: i64,
}

impl InterstitialConfig {
    /// Extract interstitial config from `device.ext.prebid.interstitial`.
    pub fn from_device(device: &Device) -> Option<Self> {
        let ext = device.ext.as_ref()?;
        let prebid = ext.get("prebid")?;
        let interstitial = prebid.get("interstitial")?;
        Some(Self {
            min_width_perc: interstitial.get("minwidthperc")?.as_i64().unwrap_or(0),
            min_height_perc: interstitial.get("minheightperc")?.as_i64().unwrap_or(0),
        })
    }
}

// ---------------------------------------------------------------------------
// processInterstitials — main entry point
// ---------------------------------------------------------------------------

/// Process interstitial impressions in a bid request, resolving sizes based
/// on device dimensions and publisher preferences.
///
/// Mirrors Go `processInterstitials()`.
pub fn process_interstitials(req: &mut BidRequest) -> Result<(), String> {
    // Extract config from device.ext.prebid.interstitial
    let config = match req.device.as_ref().and_then(InterstitialConfig::from_device) {
        Some(c) => c,
        None => {
            // No interstitial config provided — nothing to do
            return Ok(());
        }
    };

    let device = req.device.clone();

    for imp in req.imp.iter_mut() {
        if imp.instl == Some(1) {
            process_interstitials_for_imp(imp, &config, device.as_ref())?;
        }
    }
    Ok(())
}

/// Resolve sizes for a single interstitial impression.
/// Mirrors Go `processInterstitialsForImp()`.
pub fn process_interstitials_for_imp(
    imp: &mut Imp,
    config: &InterstitialConfig,
    device: Option<&Device>,
) -> Result<(), String> {
    // Custom interstitial support is only for banner requests
    let banner = match imp.banner.as_mut() {
        Some(b) => b,
        None => return Ok(()),
    };

    let (mut max_width, mut max_height): (i64, i64) = (0, 0);

    // Read max size from banner.format[0] if present
    if let Some(formats) = &banner.format {
        if let Some(first) = formats.first() {
            max_width = first.w.unwrap_or(0) as i64;
            max_height = first.h.unwrap_or(0) as i64;
        }
    }

    // If size is 1x1 (or smaller), use device dimensions
    if max_width < 2 && max_height < 2 {
        let device = match device {
            Some(d) => d,
            None => {
                return Err(format!(
                    "Unable to read max interstitial size for Imp id={} (No Device and no Format objects)",
                    imp.id
                ));
            }
        };
        max_width = device.w.unwrap_or(0) as i64;
        max_height = device.h.unwrap_or(0) as i64;
    }

    let min_width = (max_width * config.min_width_perc) / 100;
    let min_height = (max_height * config.min_height_perc) / 100;

    let new_formats = gen_interstitial_format(min_width, max_width, min_height, max_height);

    if new_formats.is_empty() {
        return Err(format!(
            "Unable to set interstitial size list for Imp id={} (No valid sizes between {}x{} and {}x{})",
            imp.id, min_width, min_height, max_width, max_height
        ));
    }

    banner.format = Some(new_formats);
    Ok(())
}

/// Generate a list of formats that fit within the min/max bounds.
/// Takes the first 10 matching sizes from the resolved list.
/// Mirrors Go `genInterstitialFormat()`.
fn gen_interstitial_format(
    min_width: i64,
    max_width: i64,
    min_height: i64,
    max_height: i64,
) -> Vec<Format> {
    let mut sizes = Vec::with_capacity(10);
    for size in RESOLVED_INTERSTITIAL_SIZES {
        let w = size.width as i64;
        let h = size.height as i64;
        if w >= min_width && w <= max_width && h >= min_height && h <= max_height {
            sizes.push(Format {
                w: Some(w as i32),
                h: Some(h as i32),
                ..Default::default()
            });
            if sizes.len() >= 10 {
                break;
            }
        }
    }
    sizes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_interstitials_no_config() {
        let mut req = BidRequest::default();
        req.imp = vec![Imp {
            id: "imp1".to_string(),
            instl: Some(1),
            banner: Some(openrtb::Banner::default()),
            ..Default::default()
        }];
        // No device — no-op
        assert!(process_interstitials(&mut req).is_ok());
    }

    #[test]
    fn test_process_interstitials_with_device_sizes() {
        let mut req = BidRequest::default();
        req.device = Some(Device {
            w: Some(1920),
            h: Some(1080),
            ext: Some(serde_json::json!({
                "prebid": {
                    "interstitial": {
                        "minwidthperc": 60,
                        "minheightperc": 60
                    }
                }
            })),
            ..Default::default()
        });
        req.imp = vec![Imp {
            id: "imp1".to_string(),
            instl: Some(1),
            banner: Some(openrtb::Banner {
                format: Some(vec![Format { w: Some(1), h: Some(1), ..Default::default() }]),
                ..Default::default()
            }),
            ..Default::default()
        }];

        let result = process_interstitials(&mut req);
        assert!(result.is_ok());

        let banner = req.imp[0].banner.as_ref().unwrap();
        let formats = banner.format.as_ref().unwrap();
        assert!(!formats.is_empty(), "should generate format sizes");

        // All sizes should be within min (60% of 1920x1080 = 1152x648) and max (1920x1080)
        for f in formats {
            let w = f.w.unwrap_or(0);
            let h = f.h.unwrap_or(0);
            assert!(w >= 1152 && w <= 1920, "width {} out of range", w);
            assert!(h >= 648 && h <= 1080, "height {} out of range", h);
        }
    }

    #[test]
    fn test_process_interstitials_not_interstitial() {
        let mut req = BidRequest::default();
        req.device = Some(Device {
            w: Some(1920),
            h: Some(1080),
            ext: Some(serde_json::json!({
                "prebid": {"interstitial": {"minwidthperc": 50, "minheightperc": 50}}
            })),
            ..Default::default()
        });
        let original_format = vec![Format { w: Some(300), h: Some(250), ..Default::default() }];
        req.imp = vec![Imp {
            id: "imp1".to_string(),
            instl: Some(0), // NOT interstitial
            banner: Some(openrtb::Banner {
                format: Some(original_format.clone()),
                ..Default::default()
            }),
            ..Default::default()
        }];

        assert!(process_interstitials(&mut req).is_ok());
        // Format should be unchanged
        let banner = req.imp[0].banner.as_ref().unwrap();
        let formats = banner.format.as_ref().unwrap();
        assert_eq!(formats.len(), 1);
        assert_eq!(formats[0].w, Some(300));
    }

    #[test]
    fn test_gen_interstitial_format_bounds() {
        let sizes = gen_interstitial_format(100, 400, 100, 400);
        // All sizes should be within 100x100 to 400x400
        for s in &sizes {
            let w = s.w.unwrap() as i64;
            let h = s.h.unwrap() as i64;
            assert!(w >= 100 && w <= 400);
            assert!(h >= 100 && h <= 400);
        }
        assert!(sizes.len() <= 10);
    }

    #[test]
    fn test_gen_interstitial_format_empty_when_no_match() {
        let sizes = gen_interstitial_format(3000, 4000, 3000, 4000);
        assert!(sizes.is_empty());
    }

    #[test]
    fn test_interstitial_config_from_device() {
        let device = Device {
            ext: Some(serde_json::json!({
                "prebid": {
                    "interstitial": {
                        "minwidthperc": 60,
                        "minheightperc": 70
                    }
                }
            })),
            ..Default::default()
        };
        let cfg = InterstitialConfig::from_device(&device).unwrap();
        assert_eq!(cfg.min_width_perc, 60);
        assert_eq!(cfg.min_height_perc, 70);
    }

    #[test]
    fn test_interstitial_config_no_ext() {
        let device = Device::default();
        assert!(InterstitialConfig::from_device(&device).is_none());
    }

    #[test]
    fn test_process_interstitials_no_banner_skipped() {
        let mut req = BidRequest::default();
        req.device = Some(Device {
            w: Some(1920),
            h: Some(1080),
            ext: Some(serde_json::json!({
                "prebid": {"interstitial": {"minwidthperc": 50, "minheightperc": 50}}
            })),
            ..Default::default()
        });
        req.imp = vec![Imp {
            id: "imp1".to_string(),
            instl: Some(1),
            video: Some(openrtb::Video::default()),
            // No banner — should skip
            ..Default::default()
        }];

        assert!(process_interstitials(&mut req).is_ok());
    }
}
