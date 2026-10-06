use pbs::adapters::lunamedia::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/lunamedia", &Adapter::new("http://rtb.lunamedia.live/?pid={{.PublisherID}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
