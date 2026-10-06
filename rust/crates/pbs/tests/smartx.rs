use pbs::adapters::smartx::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/smartx", &Adapter::new("https://bid.smartclip.net/bid/1005"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
