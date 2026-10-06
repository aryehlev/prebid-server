use pbs::adapters::adtrgtme::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://localhost/ssp");
    let failures = run_json_bidder_test("tests/fixtures/adtrgtme", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
