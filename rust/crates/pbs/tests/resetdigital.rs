use pbs::adapters::resetdigital::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/resetdigital", &Adapter::new("https://test.com").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
