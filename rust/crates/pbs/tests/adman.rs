use pbs::adapters::adman::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adman", &Adapter::new("http://pub.admanmedia.com/?c=o&m=ortb"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
