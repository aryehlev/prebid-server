use pbs::adapters::pwbid::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/pwbid", &Adapter::new("https://bidder.east2.pubwise.io/bid/pubwisedirect"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
