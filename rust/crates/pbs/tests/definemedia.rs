use pbs::adapters::definemedia::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/definemedia", &Adapter::new("https://rtb.conative.network/openrtb2/auction"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
