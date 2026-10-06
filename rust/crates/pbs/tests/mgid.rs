use pbs::adapters::mgid::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/mgid", &Adapter::new("https://prebid.mgid.com/prebid/"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
