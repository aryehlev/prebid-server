use pbs::adapters::bidtheatre::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/bidtheatre", &Adapter::new("http://any.url"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
