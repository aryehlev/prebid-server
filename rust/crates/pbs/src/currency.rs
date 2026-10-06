//! Go `currency.Rates` as far as adapters use it: `GetRate` with ISO-4217 validation, the
//! reverse pair and the intermediate-currency fallback.

use std::collections::HashMap;

use crate::errortypes::BidderError;

/// ISO-4217 codes `golang.org/x/text/currency` (v0.41.0, the seller's pin) recognises.
const ISO_CODES: &[&str] = &[
    "ADP", "AED", "AFA", "AFN", "ALK", "ALL", "AMD", "ANG", "AOA", "AOK",
    "AON", "AOR", "ARA", "ARL", "ARM", "ARP", "ARS", "ATS", "AUD", "AWG",
    "AZM", "AZN", "BAD", "BAM", "BAN", "BBD", "BDT", "BEC", "BEF", "BEL",
    "BGL", "BGM", "BGN", "BGO", "BHD", "BIF", "BMD", "BND", "BOB", "BOL",
    "BOP", "BOV", "BRB", "BRC", "BRE", "BRL", "BRN", "BRR", "BRZ", "BSD",
    "BTN", "BUK", "BWP", "BYB", "BYN", "BYR", "BZD", "CAD", "CDF", "CHE",
    "CHF", "CHW", "CLE", "CLF", "CLP", "CNH", "CNX", "CNY", "COP", "COU",
    "CRC", "CSD", "CSK", "CUC", "CUP", "CVE", "CYP", "CZK", "DDM", "DEM",
    "DJF", "DKK", "DOP", "DZD", "ECS", "ECV", "EEK", "EGP", "ERN", "ESA",
    "ESB", "ESP", "ETB", "EUR", "FIM", "FJD", "FKP", "FRF", "GBP", "GEK",
    "GEL", "GHC", "GHS", "GIP", "GMD", "GNF", "GNS", "GQE", "GRD", "GTQ",
    "GWE", "GWP", "GYD", "HKD", "HNL", "HRD", "HRK", "HTG", "HUF", "IDR",
    "IEP", "ILP", "ILR", "ILS", "INR", "IQD", "IRR", "ISJ", "ISK", "ITL",
    "JMD", "JOD", "JPY", "KES", "KGS", "KHR", "KMF", "KPW", "KRH", "KRO",
    "KRW", "KWD", "KYD", "KZT", "LAK", "LBP", "LKR", "LRD", "LSL", "LTL",
    "LTT", "LUC", "LUF", "LUL", "LVL", "LVR", "LYD", "MAD", "MAF", "MCF",
    "MDC", "MDL", "MGA", "MGF", "MKD", "MKN", "MLF", "MMK", "MNT", "MOP",
    "MRO", "MTL", "MTP", "MUR", "MVP", "MVR", "MWK", "MXN", "MXP", "MXV",
    "MYR", "MZE", "MZM", "MZN", "NAD", "NGN", "NIC", "NIO", "NLG", "NOK",
    "NPR", "NZD", "OMR", "PAB", "PEI", "PEN", "PES", "PGK", "PHP", "PKR",
    "PLN", "PLZ", "PTE", "PYG", "QAR", "RHD", "ROL", "RON", "RSD", "RUB",
    "RUR", "RWF", "SAR", "SBD", "SCR", "SDD", "SDG", "SDP", "SEK", "SGD",
    "SHP", "SIT", "SKK", "SLL", "SOS", "SRD", "SRG", "SSP", "STD", "STN",
    "SUR", "SVC", "SYP", "SZL", "THB", "TJR", "TJS", "TMM", "TMT", "TND",
    "TOP", "TPE", "TRL", "TRY", "TTD", "TWD", "TZS", "UAH", "UAK", "UGS",
    "UGX", "USD", "USN", "USS", "UYI", "UYP", "UYU", "UZS", "VEB", "VEF",
    "VND", "VNN", "VUV", "WST", "XAF", "XAG", "XAU", "XBA", "XBB", "XBC",
    "XBD", "XCD", "XDR", "XEU", "XFO", "XFU", "XOF", "XPD", "XPF", "XPT",
    "XRE", "XSU", "XTS", "XUA", "XXX", "YDD", "YER", "YUD", "YUM", "YUN",
    "YUR", "ZAL", "ZAR", "ZMK", "ZMW", "ZRN", "ZRZ", "ZWD", "ZWL", "ZWR",
];

/// Go `currency.ParseISO` on the text of a code: `Err` for a malformed or unknown code, else the
/// upper-cased code. `XXX` parses to the zero unit, whose `String()` is `"XXX"`.
fn parse_iso(s: &str) -> Result<String, BidderError> {
    if s.len() != 3 || !s.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(BidderError::other("currency: tag is not well-formed"));
    }
    let code = s.to_ascii_uppercase();
    if code == "XXX" || ISO_CODES.contains(&code.as_str()) {
        Ok(code)
    } else {
        Err(BidderError::other("currency: tag is not a recognized currency"))
    }
}

/// Go `currency.Rates` (`Conversions` nil is `None`).
#[derive(Debug, Clone, Default)]
pub struct Conversions {
    rates: Option<HashMap<String, HashMap<String, f64>>>,
}

impl Conversions {
    /// Go `currency.NewRates(conversionRates)`.
    pub fn new(rates: HashMap<String, HashMap<String, f64>>) -> Self {
        Self { rates: Some(rates) }
    }

    /// Go `Rates.GetRate`.
    pub fn get_rate(&self, from: &str, to: &str) -> Result<f64, BidderError> {
        let from = parse_iso(from)?;
        let to = parse_iso(to)?;
        if from == to {
            return Ok(1.0);
        }
        let Some(rates) = &self.rates else {
            return Err(BidderError::other("rates are nil"));
        };
        if let Some(rate) = rates.get(&from).and_then(|m| m.get(&to)) {
            return Ok(*rate);
        }
        if let Some(rate) = rates.get(&to).and_then(|m| m.get(&from)) {
            return Ok(1.0 / rate);
        }
        // Go `FindIntermediateConversionRate`. Go ranges over a map, so with several valid
        // intermediates the pick is random there too.
        for conversions in rates.values() {
            if let (Some(to_rate), Some(from_rate)) = (conversions.get(&to), conversions.get(&from)) {
                return Ok(to_rate / from_rate);
            }
        }
        Err(BidderError::other(format!(
            "Currency conversion rate not found: '{from}' => '{to}'"
        )))
    }
}
