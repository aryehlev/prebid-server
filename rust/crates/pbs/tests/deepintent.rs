use pbs::adapters::deepintent::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/deepintent", &Adapter::new("https://prebid.deepintent.com/prebid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
