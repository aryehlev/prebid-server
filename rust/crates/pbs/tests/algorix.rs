use pbs::adapters::algorix::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://{{.Host}}.test.com?sid={{.SourceId}}&token={{.AccountID}}").unwrap();
    let failures = run_json_bidder_test("tests/fixtures/algorix", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
