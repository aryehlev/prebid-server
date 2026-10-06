use pbs::adapters::decenterads::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/decenterads", &Adapter::new("http://example.com/?c=o&m=ortb"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
