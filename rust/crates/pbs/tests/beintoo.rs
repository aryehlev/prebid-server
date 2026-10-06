use pbs::adapters::beintoo::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/beintoo", &Adapter::new("https://ib.beintoo.com"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
