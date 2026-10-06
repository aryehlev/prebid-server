use pbs::adapters::medianet::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/medianet", &Adapter::new("https://example.media.net/rtb/prebid", "http://localhost:8080/extrnal_url"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
