use pbs::adapters::rubicon::Adapter;
use pbs::config;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let cfg = config::Adapter {
        endpoint: "uri".into(),
        xapi: config::AdapterXapi {
            username: "xuser".into(),
            password: "xpass".into(),
            tracker: "pbs-test-tracker".into(),
        },
        ..Default::default()
    };
    let server = config::Server { external_url: "http://hosturl.com".into(), gvl_id: 1, data_center: "2".into() };
    let failures = run_json_bidder_test("tests/fixtures/rubicon", &Adapter::new(&cfg, &server));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
