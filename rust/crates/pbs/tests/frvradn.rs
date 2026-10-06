use pbs::adapters::frvradn::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://fran.frvr.com/api/v1/openrtb").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/frvradn", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
