use pbs::adapters::blue::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/blue", &Adapter::new("https://foo.io/?src=prebid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
