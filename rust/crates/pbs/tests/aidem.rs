use pbs::adapters::aidem::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("https://fakezero.aidemsrv.com/ortb/v2.6/bid/request?billing_id={{.PublisherID}}").expect("Builder returned unexpected error");
    let failures = run_json_bidder_test("tests/fixtures/aidem", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
