use pbs::adapters::pulsepoint::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://bidder.pulsepoint.com").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/pulsepoint", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
