use pbs::adapters::criteo::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://ssp-bidder.criteo.com/openrtb/pbs/auction/request?profile=230").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/criteo", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
