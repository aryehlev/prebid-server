use pbs::adapters::sovrn_xsp::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/sovrnXsp", &Adapter::new("http://xsp.lijit.com/json/rtb/prebid/server"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
