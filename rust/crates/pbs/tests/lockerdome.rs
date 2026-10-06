use pbs::adapters::lockerdome::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/lockerdome", &Adapter::new("https://lockerdome.com/ladbid/prebidserver/openrtb2"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
