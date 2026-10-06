use pbs::adapters::ads_interactive::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/ads_interactive", &Adapter::new("https://example.com"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
