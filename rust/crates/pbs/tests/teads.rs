use pbs::adapters::teads::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/teads", &Adapter::new("https://psrv.teads.tv/prebid-server/bid-request").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
