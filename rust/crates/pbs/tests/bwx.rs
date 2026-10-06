use pbs::adapters::bwx::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/bwx", &Adapter::new("http://rtb.boldwin.live/?pid={{.SourceId}}&host={{.Host}}&pbs=1").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
