use pbs::adapters::imds::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("http://pbs.technoratimedia.com/openrtb/bids/{{.AccountID}}?src={{.SourceId}}&adapter=imds").expect("Builder");
    let failures = run_json_bidder_test("tests/fixtures/imds", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
