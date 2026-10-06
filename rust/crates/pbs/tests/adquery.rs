use pbs::adapters::adquery::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adquery", &Adapter::new("https://bidder2.adquery.io/prebid/bid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
