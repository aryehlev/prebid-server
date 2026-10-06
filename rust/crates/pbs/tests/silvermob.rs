use pbs::adapters::silvermob::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("http://{{.Host}}.example.com/api/dsp/bid/{{.ZoneID}}").expect("Builder returned unexpected error");
    let failures = run_json_bidder_test("tests/fixtures/silvermob", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
