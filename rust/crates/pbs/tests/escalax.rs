use pbs::adapters::escalax::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/escalax", &Adapter::new("http://bidder_us.escalax.io/?partner={{.SourceId}}&token={{.AccountID}}&type=pbs").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
