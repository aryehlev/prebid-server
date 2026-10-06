use pbs::adapters::rtbhouse::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://localhost/prebid_server");
    let failures = run_json_bidder_test("tests/fixtures/rtbhouse", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
