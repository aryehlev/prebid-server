use pbs::adapters::feedad::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/feedad", &Adapter::new("https://example.com/1/prebid/requests"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
