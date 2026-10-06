use pbs::adapters::smartrtb::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("http://market-east.smrtb.com/json/publisher/rtb?pubid=test").expect("Builder");
    let failures = run_json_bidder_test("tests/fixtures/smartrtb", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
