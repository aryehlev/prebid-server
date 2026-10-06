use pbs::adapters::limelight_digital::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://test.ortb.net/openrtb/{{.PublisherID}}?host={{.Host}}").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/limelightDigital", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
