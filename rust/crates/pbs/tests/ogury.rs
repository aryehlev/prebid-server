use pbs::adapters::ogury::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/ogury", &Adapter::new("http://ogury.example.com"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
