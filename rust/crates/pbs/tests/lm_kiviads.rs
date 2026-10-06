use pbs::adapters::lm_kiviads::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/lm_kiviads", &Adapter::new("http://pbs.kiviads.live/?pid={{.SourceId}}&host={{.Host}}")
        .unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}").is_err());
}
