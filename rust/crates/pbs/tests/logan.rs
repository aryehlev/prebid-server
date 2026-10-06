use pbs::adapters::logan::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/logan", &Adapter::new("http://endpoint1.logan.com/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
