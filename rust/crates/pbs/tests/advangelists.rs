use pbs::adapters::advangelists::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://nep.advangelists.com/xp/get?pubid={{.PublisherID}}").unwrap();
    let failures = run_json_bidder_test("tests/fixtures/advangelists", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
