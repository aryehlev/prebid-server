use pbs::adapters::kargo::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://example.com/bid").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/kargo", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
