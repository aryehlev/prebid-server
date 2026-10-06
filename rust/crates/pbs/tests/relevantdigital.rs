use pbs::adapters::relevantdigital::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/relevantdigital", &Adapter::new("https://{{.Host}}.relevant-digital.com/openrtb2/auction").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
