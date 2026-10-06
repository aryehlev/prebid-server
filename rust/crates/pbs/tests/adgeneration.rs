use pbs::adapters::adgeneration::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://d.socdm.com/adsv/v1").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/adgeneration", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
