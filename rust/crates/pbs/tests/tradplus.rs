use pbs::adapters::tradplus::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://{{.ZoneID}}adx.tradplusad.com/{{.AccountID}}/pserver").unwrap();
    let failures = run_json_bidder_test("tests/fixtures/tradplus", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
