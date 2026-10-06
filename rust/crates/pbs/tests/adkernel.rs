use pbs::adapters::adkernel::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adkernel", &Adapter::new("http://pbs.adksrv.com/hb?zone={{.ZoneID}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
