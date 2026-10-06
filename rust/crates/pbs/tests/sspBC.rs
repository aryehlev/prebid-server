use pbs::adapters::ssp_bc::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/sspBC", &Adapter::new("http://ssp.wp.test/bidder/").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
