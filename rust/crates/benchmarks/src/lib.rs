//! Benchmarks crate

/// Returns a canned OpenRTB bid request body used by the microbenchmarks.
///
/// The payload is intentionally representative of a small, but realistic,
/// `/openrtb2/auction` request: a single banner imp, a site object, a user
/// object with a couple of eids, and a regs/ext block. It is static so that
/// benchmarks measure parse cost rather than allocator warm-up.
pub fn sample_request_bytes() -> Vec<u8> {
    const SAMPLE: &str = r#"{
        "id": "req-benchmark-0001",
        "tmax": 500,
        "at": 1,
        "cur": ["USD"],
        "imp": [
            {
                "id": "imp-1",
                "banner": {
                    "format": [
                        {"w": 300, "h": 250},
                        {"w": 300, "h": 600}
                    ],
                    "w": 300,
                    "h": 250
                },
                "bidfloor": 0.10,
                "bidfloorcur": "USD",
                "secure": 1,
                "ext": {
                    "prebid": {
                        "bidder": {
                            "appnexus": {"placementId": 123456},
                            "rubicon":  {"accountId": 1001, "siteId": 2002, "zoneId": 3003}
                        }
                    }
                }
            }
        ],
        "site": {
            "id": "site-1",
            "domain": "example.com",
            "page": "https://example.com/article/1",
            "publisher": {"id": "pub-1", "domain": "example.com"}
        },
        "device": {
            "ua": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36",
            "ip": "192.0.2.1",
            "devicetype": 2,
            "os": "Linux",
            "language": "en"
        },
        "user": {
            "id": "user-abc",
            "buyeruid": "buyer-xyz",
            "ext": {
                "eids": [
                    {"source": "adserver.org", "uids": [{"id": "tdid-1"}]},
                    {"source": "liveramp.com", "uids": [{"id": "lr-1"}]}
                ]
            }
        },
        "regs": {
            "coppa": 0,
            "ext": {"gdpr": 0, "us_privacy": "1YNN"}
        },
        "ext": {
            "prebid": {
                "targeting": {"pricegranularity": "medium"},
                "cache":     {"bids": {}}
            }
        }
    }"#;
    SAMPLE.as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_request_bytes_is_non_empty() {
        let bytes = sample_request_bytes();
        assert!(!bytes.is_empty());
    }
}
