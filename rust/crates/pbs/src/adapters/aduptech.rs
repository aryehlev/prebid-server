//! Go `adapters/aduptech/aduptech.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::currency::Conversions;
use crate::errortypes::BidderError;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, MarkupType};

pub struct Adapter {
    endpoint: String,
    target_currency: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtraInfo {
    target_currency: String,
}

impl Adapter {
    /// Go `Builder`; `extra_adapter_info` is `config.Adapter.ExtraAdapterInfo`.
    pub fn new(endpoint: impl Into<String>, extra_adapter_info: &str) -> Result<Self, BidderError> {
        let extra: ExtraInfo = jsonutil::unmarshal(extra_adapter_info.as_bytes())
            .map_err(|e| BidderError::other(format!("invalid extra info: {e}")))?;
        if extra.target_currency.is_empty() {
            return Err(BidderError::other("invalid extra info: TargetCurrency is empty, pls check"));
        }
        // Go `currency.ParseISO`; `get_rate(x, x)` validates the code and nothing else.
        if Conversions::default().get_rate(&extra.target_currency, &extra.target_currency).is_err() {
            return Err(BidderError::other(format!(
                "invalid extra info: invalid TargetCurrency {}, pls check",
                extra.target_currency
            )));
        }
        Ok(Self { endpoint: endpoint.into(), target_currency: extra.target_currency.to_ascii_uppercase() })
    }

    fn convert_currency(&self, value: f64, cur: &str, req_info: &ExtraRequestInfo) -> Result<f64, BidderError> {
        match req_info.convert_currency(value, cur, &self.target_currency) {
            Ok(v) => Ok(v),
            Err(e) => {
                // Go `errors.As(err, &ConversionNotFoundError)`.
                if !e.message().starts_with("Currency conversion rate not found:") {
                    return Err(e);
                }
                let usd = req_info.convert_currency(value, cur, "USD").map_err(|e| {
                    BidderError::other(format!(
                        "Currency conversion rate not found from '{cur}' to '{}'. Error converting from '{cur}' to 'USD': {e}",
                        self.target_currency
                    ))
                })?;
                req_info.convert_currency(usd, "USD", &self.target_currency).map_err(|e| {
                    BidderError::other(format!(
                        "Currency conversion rate not found from '{cur}' to '{}'. Error converting from 'USD' to '{}': {e}",
                        self.target_currency, self.target_currency
                    ))
                })
            }
        }
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut request = request.clone();
        for imp in request.imp.iter_mut() {
            if imp.bidfloor > 0.0
                && !imp.bidfloorcur.is_empty()
                && imp.bidfloorcur.to_uppercase() != self.target_currency
            {
                match self.convert_currency(imp.bidfloor, &imp.bidfloorcur, req_info) {
                    Ok(v) => {
                        imp.bidfloorcur = self.target_currency.clone();
                        imp.bidfloor = v;
                    }
                    Err(e) => return (vec![], vec![e]),
                }
            }
        }
        let body = match crate::go_json::to_vec(&request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers: Default::default(),
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur.clone();
        let mut errs = Vec::new();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match bid.mtype {
                    MarkupType::NATIVE => bid_response.bids.push(TypedBid::new(bid, BidType::Native)),
                    MarkupType::BANNER => bid_response.bids.push(TypedBid::new(bid, BidType::Banner)),
                    m => errs.push(BidderError::bad_server_response(format!("Unknown markup type: {}", m.0))),
                }
            }
        }
        (Some(bid_response), errs)
    }
}
