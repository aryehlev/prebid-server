use pbs::adapters::smarthub::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://prebid.example.com/pbserver?partnerName={{.Host}}&seat={{.AccountID}}&token={{.SourceId}}").unwrap();
    let failures = run_json_bidder_test("tests/fixtures/smarthub", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
