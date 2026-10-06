use pbs::adapters::mobfoxpb::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/mobfoxpb", &Adapter::new("http://example.com/?c=__route__&m=__method__&key=__key__"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
