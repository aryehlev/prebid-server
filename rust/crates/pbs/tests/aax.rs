use pbs::adapters::aax::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/aax", &Adapter::new("https://example.aax.media/rtb/prebid", "http://localhost:8080/extrnal_url"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
