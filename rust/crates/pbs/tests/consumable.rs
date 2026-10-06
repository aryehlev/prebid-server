use pbs::adapters::consumable::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/consumable", &Adapter::new("https://e.serverbid.com"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
