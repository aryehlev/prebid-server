use pbs::adapters::openweb::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/openweb", &Adapter::new("https://pbs.openwebmp.com/pbs"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
