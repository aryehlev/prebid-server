use pbs::adapters::bliink::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/bliink", &Adapter::new("http://biddertest.url/bid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
