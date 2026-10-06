use pbs::adapters::connatix::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/connatix", &Adapter::new("http://example.com"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
