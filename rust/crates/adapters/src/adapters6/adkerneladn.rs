use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct AdkernelAdnAdapter {
    pub endpoint: String,
}

impl AdkernelAdnAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ExtImpAdkernelAdn {
    pub publisher_id: i64,
    pub host: String,
}

fn get_impression_ext(imp: &openrtb::Imp) -> Result<ExtImpAdkernelAdn, BidderError> {
    let bidder = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .ok_or_else(|| BidderError::BadInput("Missing imp.ext.bidder".to_string()))?;

    let publisher_id = bidder.get("pubId")
        .or_else(|| bidder.get("publisherId"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let host = bidder.get("host")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(ExtImpAdkernelAdn { publisher_id, host })
}

fn validate_impression(imp: &openrtb::Imp, ext: &ExtImpAdkernelAdn) -> Result<(), BidderError> {
    if ext.publisher_id < 1 {
        return Err(BidderError::BadInput(format!(
            "Invalid pubId value. Ignoring imp id={}", imp.id
        )));
    }
    if imp.video.is_none() && imp.banner.is_none() {
        return Err(BidderError::BadInput(format!(
            "Invalid imp with id={}. Expected imp.banner or imp.video", imp.id
        )));
    }
    Ok(())
}

/// Modify imp to comply with adkernelAdn platform requirements:
/// - Clear ext
/// - For banner: ensure w/h set from format[0] if needed, clear video/native/audio
/// - For video: clear banner/audio/native
fn compat_impression(imp: &mut openrtb::Imp) -> Result<(), BidderError> {
    imp.ext = None;

    if let Some(banner) = &imp.banner {
        // Ensure w/h are set
        if banner.w.is_none() && banner.h.is_none() {
            let formats = banner.format.as_deref().unwrap_or(&[]);
            if formats.is_empty() {
                return Err(BidderError::BadInput(
                    "Expected at least one banner.format entry or explicit w/h".to_string()
                ));
            }
            let format = &formats[0];
            let w = format.w;
            let h = format.h;
            let mut banner_copy = banner.clone();
            banner_copy.format = Some(formats[1..].to_vec());
            banner_copy.w = w;
            banner_copy.h = h;
            imp.banner = Some(banner_copy);
        }
        imp.video = None;
        imp.native = None;
        imp.audio = None;
    } else if imp.video.is_some() {
        imp.banner = None;
        imp.audio = None;
        imp.native = None;
    } else {
        return Err(BidderError::BadInput("Unsupported impression has been received".to_string()));
    }

    Ok(())
}

fn get_media_type_for_imp_id(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id && imp.banner.is_some() {
            return BidType::Banner;
        }
    }
    BidType::Video
}

impl Bidder for AdkernelAdnAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impression in the bid request".to_string())]);
        }

        let mut errs = Vec::new();

        // Parse and validate impressions, group by (publisher_id, host)
        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();
        let mut valid_exts: Vec<ExtImpAdkernelAdn> = Vec::new();

        for imp in &request.imp {
            let ext = match get_impression_ext(imp) {
                Ok(e) => e,
                Err(e) => { errs.push(e); continue; }
            };
            if let Err(e) = validate_impression(imp, &ext) {
                errs.push(e); continue;
            }
            valid_imps.push(imp.clone());
            valid_exts.push(ext);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        // Group imps by (publisher_id, host)
        let mut pub2imps: HashMap<ExtImpAdkernelAdn, Vec<openrtb::Imp>> = HashMap::new();
        for (idx, mut imp) in valid_imps.into_iter().enumerate() {
            match compat_impression(&mut imp) {
                Ok(_) => {}
                Err(e) => { errs.push(e); continue; }
            }
            let ext = valid_exts[idx].clone();
            pub2imps.entry(ext).or_default().push(imp);
        }

        if pub2imps.is_empty() {
            return (vec![], errs);
        }

        let mut requests = Vec::new();

        for (params, imps) in pub2imps {
            // Build request copy
            let mut req = request.clone();
            req.imp = imps.clone();

            // Clear publisher info and domain from site
            if let Some(site) = req.site.as_mut() {
                site.publisher = None;
                site.domain = None;
            }
            if let Some(app) = req.app.as_mut() {
                app.publisher = None;
            }

            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            // Resolve endpoint: replace {{.PublisherID}} with publisher_id
            let uri = self.endpoint.replace("{{.PublisherID}}", &params.publisher_id.to_string());

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());
            headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids: get_imp_ids(&req.imp),
            });
        }

        (requests, errs)
    }

    fn make_bids(
        &self,
        internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected http status code: {}", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Bad server response: {}", e))])?;

        if bid_resp.seatbid.len() != 1 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Invalid SeatBids count: {}", bid_resp.seatbid.len()
            ))]);
        }

        let seat_bid = &bid_resp.seatbid[0];
        let mut result = BidderResponse::with_capacity(seat_bid.bid.len());

        for bid in seat_bid.bid.clone() {
            let bid_type = get_media_type_for_imp_id(&bid.impid, &internal.imp);
            result.bids.push(TypedBid::new(bid, bid_type));
        }

        Ok(result)
    }
}
