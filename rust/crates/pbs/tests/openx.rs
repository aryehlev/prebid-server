use pbs::adapters::openx::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/openx", &Adapter::new("http://rtb.openx.net/prebid", "openx"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
