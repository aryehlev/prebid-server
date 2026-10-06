use pbs::adapters::logicad::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/logicad", &Adapter::new("https://localhost/adrequest/prebidserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
