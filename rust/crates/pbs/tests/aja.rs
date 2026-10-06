use pbs::adapters::aja::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/aja", &Adapter::new("https://localhost/bid/4"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
