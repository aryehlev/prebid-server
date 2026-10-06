use pbs::adapters::across33::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/33across", &Adapter::new("http://ssc.33across.com"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
