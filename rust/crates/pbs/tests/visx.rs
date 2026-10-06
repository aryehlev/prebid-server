use pbs::adapters::visx::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/visx", &Adapter::new("http://localhost/prebid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
