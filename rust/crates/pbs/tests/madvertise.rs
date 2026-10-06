use pbs::adapters::madvertise::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("https://mobile.mng-ads.com/bidrequest{{.ZoneID}}").expect("Builder");
    let failures = run_json_bidder_test("tests/fixtures/madvertise", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn malformed_endpoint_template_fails_to_build() {
    // Go `TestEndpointTemplateMalformed`: `assert.Error(t, buildErr)`.
    assert!(Adapter::new("{{Malformed}}").is_err());
}
