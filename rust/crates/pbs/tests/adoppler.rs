use pbs::adapters::adoppler::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adoppler", &Adapter::new("http://{{.AccountID}}.trustedmarketplace.com/processHeaderBid/{{.AdUnit}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
