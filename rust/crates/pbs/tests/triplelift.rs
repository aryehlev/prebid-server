use pbs::adapters::triplelift::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/triplelift", &Adapter::new("http://tlx.3lift.net/s2s/auction?sra=1&supplier_id=20"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
