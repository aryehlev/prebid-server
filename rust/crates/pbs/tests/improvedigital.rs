use pbs::adapters::improvedigital::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/improvedigital", &Adapter::new("http://localhost/{PublisherId}pbs"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
