use pbs::adapters::mabidder::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/mabidder", &Adapter::new("https://prebid.ecdrsvc.com/pbs"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
