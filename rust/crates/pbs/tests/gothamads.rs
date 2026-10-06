use pbs::adapters::gothamads::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/gothamads", &Adapter::new("http://us-e-node1.gothamads.com/?pass={{.AccountID}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
