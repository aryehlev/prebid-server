use pbs::adapters::thetradedesk::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://direct.adsrvr.org/bid/bidder/{{.SupplyId}}", "ttd").unwrap();
    let failures = run_json_bidder_test("tests/fixtures/thetradedesk", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
