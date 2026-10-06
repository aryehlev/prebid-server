use pbs::adapters::globalsun::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("http://example.com/pserver");
    let failures = run_json_bidder_test("tests/fixtures/globalsun", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
