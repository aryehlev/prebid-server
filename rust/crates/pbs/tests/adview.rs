use pbs::adapters::adview::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adview", &Adapter::new("https://bid.adview.com/agent/thirdAdxService/{{.AccountID}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
