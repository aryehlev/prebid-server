use pbs::adapters::kiviads::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/kiviads", &Adapter::new("http://endpoint1.kiviads.com/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
