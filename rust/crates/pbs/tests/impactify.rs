use pbs::adapters::impactify::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/impactify", &Adapter::new("https://sonic.impactify.media/bidder"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
