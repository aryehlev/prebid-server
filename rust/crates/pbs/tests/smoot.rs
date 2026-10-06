use pbs::adapters::smoot::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/smoot", &Adapter::new("https://fake.test.io/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
