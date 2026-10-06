use pbs::adapters::avocet::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/avocet", &Adapter::new("https://bid.staging.avct.cloud/ortb/bid/5e722ee9bd6df11d063a8013"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
