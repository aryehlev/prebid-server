use pbs::adapters::cpmstar::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/cpmstar", &Adapter::new("//host"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
