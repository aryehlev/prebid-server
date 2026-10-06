use pbs::adapters::nobid::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/nobid", &Adapter::new("http://ads.servenobid.com/ortb_adreq?tek=pbs"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
