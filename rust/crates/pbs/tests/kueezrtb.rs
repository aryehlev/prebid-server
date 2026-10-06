use pbs::adapters::kueezrtb::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/kueezrtb", &Adapter::new("http://prebidsrvr.kueezrtb.com/openrtb/"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
