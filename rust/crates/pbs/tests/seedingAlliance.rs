use pbs::adapters::seeding_alliance::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/seedingAlliance", &Adapter::new("https://mockup.seeding-alliance.de/?ssp={{.AccountID}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
