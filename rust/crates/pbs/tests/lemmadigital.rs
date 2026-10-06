use pbs::adapters::lemmadigital::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("https://test.lemmaurl.com/lemma/servad?src=prebid&pid={{.PublisherID}}&aid={{.AdUnit}}").expect("Builder");
    let failures = run_json_bidder_test("tests/fixtures/lemmadigital", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
