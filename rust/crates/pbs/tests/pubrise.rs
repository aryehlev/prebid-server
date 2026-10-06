use pbs::adapters::pubrise::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/pubrise", &Adapter::new("https://backend.pubrise.ai/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
