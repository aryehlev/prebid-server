use pbs::adapters::adrino::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adrino", &Adapter::new("https://prd-prebid-bidder.adrino.io/openrtb/bid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
