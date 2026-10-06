use pbs::adapters::zeta_global_ssp::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("https://ssp.disqus.com/bid/prebid-server?sid={{.AccountID}}").expect("Builder returned unexpected error");
    let failures = run_json_bidder_test("tests/fixtures/zeta_global_ssp", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
