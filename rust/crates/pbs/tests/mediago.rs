use pbs::adapters::mediago::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/mediago", &Adapter::new("https://REGION.mediago.io/api/bid?tn={{.AccountID}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
