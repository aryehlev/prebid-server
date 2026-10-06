use pbs::adapters::aceex::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("http://us.example.com/bid?uqhash={{.AccountID}}").expect("Builder returned unexpected error");
    let failures = run_json_bidder_test("tests/fixtures/aceex", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
