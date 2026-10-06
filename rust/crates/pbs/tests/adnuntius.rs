use pbs::adapters::adnuntius::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    // Go's test pins the clock to 2016-01-01 12:30:15 UTC, so tzo is 0.
    let bidder = Adapter::new("http://whatever.url", "http://gdpr.url").with_tz_offset_seconds(0);
    let failures = run_json_bidder_test("tests/fixtures/adnuntius", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
