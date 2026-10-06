use pbs::adapters::tappx::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/tappx", &Adapter::new("http://{{.Host}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
