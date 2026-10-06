use pbs::adapters::emtv::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/emtv", &Adapter::new("http://example.com/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
