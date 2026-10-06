use pbs::adapters::zmaticoo::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/zmaticoo", &Adapter::new("https://bid.zmaticoo.com/prebid/bid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
