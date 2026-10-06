use pbs::adapters::smartadserver::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/smartadserver", &Adapter::new("https://ssb.smartadserver.com"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
