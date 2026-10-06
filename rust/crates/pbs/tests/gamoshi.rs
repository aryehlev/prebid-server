use pbs::adapters::gamoshi::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://rtb.gamoshi.io");
    let failures = run_json_bidder_test("tests/fixtures/gamoshi", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
