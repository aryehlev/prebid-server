use pbs::adapters::loopme::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/loopme", &Adapter::new("http://loopme.example.com"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
