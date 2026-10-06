use pbs::adapters::screencore::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("http://h1.screencore.io/?kp={{.AccountID}}&kn={{.SourceId}}").expect("Builder returned unexpected error");
    let failures = run_json_bidder_test("tests/fixtures/screencore", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
