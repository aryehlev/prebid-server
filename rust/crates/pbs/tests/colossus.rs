use pbs::adapters::colossus::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/colossus", &Adapter::new("http://example.com/?c=o&m=rtb"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
