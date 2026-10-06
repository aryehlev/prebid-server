use pbs::adapters::smrtconnect::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("http://test.smrtconnect.com/openrtb2/auction?supply_id={{.SupplyId}}").expect("Builder");
    let failures = run_json_bidder_test("tests/fixtures/smrtconnect", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
