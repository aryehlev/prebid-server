use pbs::adapters::nativo::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/nativo", &Adapter::new("https://foo.io/?src=prebid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
