use pbs::adapters::oms::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://rt.marphezis.com/pbs").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/oms", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
