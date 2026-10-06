use pbs::adapters::yahoo_ads::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/yahooAds", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
