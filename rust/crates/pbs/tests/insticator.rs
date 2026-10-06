use pbs::adapters::insticator::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/insticator", &Adapter::new("https://insticator.example.com/v1/pbs"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
