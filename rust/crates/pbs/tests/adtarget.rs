use pbs::adapters::adtarget::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adtarget", &Adapter::new("http://ghb.console.adtarget.com.tr/pbs/ortb"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
