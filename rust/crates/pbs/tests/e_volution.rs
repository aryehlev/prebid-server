use pbs::adapters::e_volution::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/e_volution", &Adapter::new("http://service.e-volution.ai/pbserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
