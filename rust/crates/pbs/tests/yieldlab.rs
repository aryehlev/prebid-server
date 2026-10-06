use pbs::adapters::yieldlab::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::with_generators(
        "https://ad.yieldlab.net/testing/",
        || "testing".to_string(),
        || "33".to_string(),
    );
    let failures = run_json_bidder_test("tests/fixtures/yieldlab", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
