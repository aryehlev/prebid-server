use pbs::adapters::operaads::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://example.com/operaads/ortb/v2/{{.PublisherID}}?ep={{.AccountID}}").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/operaads", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
