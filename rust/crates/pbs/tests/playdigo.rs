use pbs::adapters::playdigo::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/playdigo", &Adapter::new("https://server.playdigo.com/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
