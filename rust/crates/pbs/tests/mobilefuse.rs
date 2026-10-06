use pbs::adapters::mobilefuse::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/mobilefuse", &Adapter::new("http://mfx.mobilefuse.com/openrtb"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
