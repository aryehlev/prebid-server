use pbs::adapters::displayio::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/displayio", &Adapter::new("https://adapter.endpoint/?macro={{.PublisherID}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
