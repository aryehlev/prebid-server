use pbs::adapters::invibes::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://{{.ZoneID}}.videostep.com/bid/ServerBidAdContent").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/invibes", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
