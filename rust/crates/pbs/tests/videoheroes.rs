use pbs::adapters::videoheroes::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/videoheroes", &Adapter::new("http://point.contextualadv.com/?t=3&partner={{.PublisherID}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
