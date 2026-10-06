use pbs::adapters::taboola::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("http://{{.MediaType}}.whatever.com/{{.GvlID}}/{{.PublisherID}}", 12)
        .expect("Builder returned unexpected error");
    let failures = run_json_bidder_test("tests/fixtures/taboola", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn empty_external_url() {
    // Go `TestEmptyExternalUrl`: a zero GvlID leaves the template value empty.
    let bidder = Adapter::new("http://whatever.com/{{.GvlID}}", 0).unwrap();
    let _ = bidder;
}

#[test]
fn endpoint_template_malformed() {
    assert!(Adapter::new("{{Malformed}}", 0).is_err());
}
