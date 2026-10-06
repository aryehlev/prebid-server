use pbs::adapters::krushmedia::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/krushmedia", &Adapter::new("http://example.com/?c=rtb&m=req&key={{.AccountID}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
