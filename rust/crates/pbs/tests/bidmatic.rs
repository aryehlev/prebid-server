use pbs::adapters::bidmatic::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://adapter.bidmatic.io/pbs/ortb");
    let failures = run_json_bidder_test("tests/fixtures/bidmatic", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
