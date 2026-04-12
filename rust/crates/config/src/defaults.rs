//! Default values for the new multi-source [`Configuration`].
//!
//! Mirrors the defaults established in Go's `config.SetupViper`. The returned
//! configuration is guaranteed to pass [`crate::validation::Configuration::validate`].

use crate::top::*;

/// Produce a fully-populated [`Configuration`] with sensible defaults.
pub fn default_configuration() -> Configuration {
    Configuration {
        host: String::new(),
        port: 8000,
        admin_port: 6060,
        external_url: String::new(),
        enable_gzip: false,
        garbage_collector_threshold: 0,
        status_response: String::new(),
        datacenter: String::new(),

        currency: Currency {
            fetch_url:
                "https://cdn.jsdelivr.net/gh/prebid/currency-file@1/latest.json".to_string(),
            fetch_interval_seconds: 1800,
            stale_rates_seconds: 0,
            default_currency: Some("USD".to_string()),
        },

        stored_requests: StoredRequests {
            backend: StoredRequestsBackend {
                r#type: "none".to_string(),
                file: None,
                http: None,
            },
            cache_events_api: false,
            http_events: None,
        },

        metrics: Metrics {
            influxdb: None,
            prometheus: None,
            disabled_metrics: DisabledMetrics::default(),
        },

        analytics: Analytics::default(),

        gdpr: Gdpr {
            enabled: true,
            default_value: "0".to_string(),
            host_vendor_id: 0,
            timeouts_ms: TimeoutsMs {
                init_vendorlist_fetches: 0,
                active_vendorlist_fetch: 0,
            },
            non_standard_publishers: Vec::new(),
            tcf2: Tcf2Config {
                enabled: true,
                purpose_one_treatment: Some(PurposeOneTreatment {
                    enabled: true,
                    access_allowed: true,
                }),
                special_feature1: Some(SpecialFeature1 {
                    enforce: true,
                    vendor_exceptions: Vec::new(),
                }),
            },
            amp_exception: false,
            eea_countries: Vec::new(),
        },

        ccpa: Ccpa { enforce: true },
        lmt: Lmt { enforce: true },

        privacy: Privacy {
            ipv4: IpMasking { anon_keep_bits: 24 },
            ipv6: IpMasking { anon_keep_bits: 56 },
        },

        host_cookie: HostCookie {
            enabled: false,
            domain: String::new(),
            family: "host-cookie".to_string(),
            cookie_name: "uids".to_string(),
            opt_out_url: String::new(),
            opt_in_url: String::new(),
            max_cookie_size_bytes: 0,
            ttl_days: 14,
        },

        cookie_sync: CookieSync {
            default_limit: 0,
            max_limit: 0,
            default_coop_sync: false,
            pri: Vec::new(),
        },

        price_floors: PriceFloors {
            enabled: false,
            fetcher: PriceFloorFetcher {
                http_client_timeout_seconds: 10,
                max_retries: 10,
                cache_size_mb: 64,
                worker: 20,
            },
        },

        debug: Debug {
            timeout_notification: TimeoutNotification {
                log: false,
                sampling_rate: 0.0,
                fail_only: false,
            },
            override_token: String::new(),
        },

        experiment: Experiment {
            adscert: AdsCert {
                mode: "off".to_string(),
                in_process: None,
                remote: None,
            },
        },

        bidder_infos: BidderInfos::default(),
    }
}
