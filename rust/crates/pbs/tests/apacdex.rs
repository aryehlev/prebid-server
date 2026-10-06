use pbs::adapters::apacdex::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/apacdex", &Adapter::new("//host"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
