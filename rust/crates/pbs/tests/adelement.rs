use pbs::adapters::adelement::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adelement", &Adapter::new("http://test.adelement.com/openrtb2/auction?supply_id={{.SupplyId}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
