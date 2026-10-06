use pbs::adapters::trafficgate::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/trafficgate", &Adapter::new("http://{{.Host}}.bc-plugin.com/?c=o&m=rtb").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
