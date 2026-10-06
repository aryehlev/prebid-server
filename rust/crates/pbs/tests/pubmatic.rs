use pbs::adapters::pubmatic::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/pubmatic", &Adapter::new("https://hbopenbid.pubmatic.com/translator?source=prebid-server"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
