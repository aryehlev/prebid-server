use pbs::adapters::smilewanted::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/smilewanted", &Adapter::new("http://example.com/go/"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
