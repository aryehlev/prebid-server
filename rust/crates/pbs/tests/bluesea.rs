use pbs::adapters::bluesea::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/bluesea", &Adapter::new("https://test.prebid.bluesea"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
