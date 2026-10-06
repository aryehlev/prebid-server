use pbs::adapters::readpeak::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/readpeak", &Adapter::new("https://dsp.readpeak.com/header/prebid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
