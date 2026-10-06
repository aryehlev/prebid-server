use pbs::adapters::adf::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adf", &Adapter::new("https://adx.adform.net/adx/openrtb"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
