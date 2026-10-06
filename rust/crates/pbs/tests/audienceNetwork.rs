use pbs::adapters::audience_network::Adapter;
use pbs::config;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let cfg = config::Adapter {
        endpoint: "https://an.facebook.com/placementbid.ortb".into(),
        platform_id: "test-platform-id".into(),
        app_secret: "test-app-secret".into(),
        ..Default::default()
    };
    let bidder = Adapter::new(&cfg).unwrap();
    let failures = run_json_bidder_test("tests/fixtures/audienceNetwork", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
