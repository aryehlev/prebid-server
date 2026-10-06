use pbs::adapters::marsmedia::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/marsmedia", &Adapter::new("http://bid306.rtbsrv.com/bidder/?bid=f3xtet"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
