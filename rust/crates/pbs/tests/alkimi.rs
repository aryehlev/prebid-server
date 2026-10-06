use pbs::adapters::alkimi::Adapter;
use pbs::testing::run_json_bidder_test;

const ENDPOINT: &str = "https://exchange.alkimi-onboarding.com/server/bid";

#[test]
fn json_samples() {
    let bidder = Adapter::new(ENDPOINT).expect("Builder returned unexpected error");
    let failures = run_json_bidder_test("tests/fixtures/alkimi", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_empty() {
    assert!(Adapter::new("").is_err());
}

#[test]
fn endpoint_malformed() {
    assert!(Adapter::new(" http://leading.space.is.invalid").is_err());
}

#[test]
fn builder() {
    assert!(Adapter::new(ENDPOINT).is_ok());
}
