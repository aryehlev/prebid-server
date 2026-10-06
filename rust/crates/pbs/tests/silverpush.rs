use pbs::adapters::silverpush::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/silverpush", &Adapter::new("http://localhost:8080/bidder/?identifier=5krH8Q"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
