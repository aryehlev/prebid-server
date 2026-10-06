use pbs::adapters::driftpixel::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/driftpixel", &Adapter::new("http://rtb.driftpixel.live/?pid={{.SourceId}}&host={{.Host}}&pbs=1")
        .unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
