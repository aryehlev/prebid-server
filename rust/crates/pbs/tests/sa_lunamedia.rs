use pbs::adapters::sa_lunamedia::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/sa_lunamedia", &Adapter::new("http://test.com/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
