use pbs::adapters::amx::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/amx", &Adapter::new("http://pbs-dev.amxrtb.com/auction/openrtb").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
