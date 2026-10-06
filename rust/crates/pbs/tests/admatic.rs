use pbs::adapters::admatic::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/admatic", &Adapter::new("http://pbs.admatic.com.tr?host={{.Host}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
