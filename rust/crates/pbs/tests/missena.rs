use pbs::adapters::missena::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("http://example.com/?t={{.PublisherID}}").expect("Builder");
    let failures = run_json_bidder_test("tests/fixtures/missena", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
