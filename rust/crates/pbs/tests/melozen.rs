use pbs::adapters::melozen::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("https://example.com/rtb/v2/bid?publisher_id={{.PublisherID}}").expect("Builder returned unexpected error");
    let failures = run_json_bidder_test("tests/fixtures/melozen", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
