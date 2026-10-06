use pbs::adapters::oraki::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/oraki", &Adapter::new("https://fake.test.io/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
