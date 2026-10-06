use pbs::adapters::loyal::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://us-east-1.loyal.app/pserver");
    let failures = run_json_bidder_test("tests/fixtures/loyal", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
