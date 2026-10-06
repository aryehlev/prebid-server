use pbs::adapters::acuityads::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://{{.Host}}.example.com/bid?token={{.AccountID}}").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/acuityads", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
