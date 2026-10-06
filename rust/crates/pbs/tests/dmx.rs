use pbs::adapters::dmx::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/dmx", &Adapter::new(""));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
