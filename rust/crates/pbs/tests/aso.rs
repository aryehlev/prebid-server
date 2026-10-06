use pbs::adapters::aso::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://srv.aso1.net/pbs/bidder?zid={{.ZoneID}}").unwrap();
    let failures = run_json_bidder_test("tests/fixtures/aso", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("zid={{ZoneID}}").is_err());
}
