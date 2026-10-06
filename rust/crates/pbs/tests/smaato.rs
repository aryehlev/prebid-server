use pbs::adapters::smaato::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    // Go test swaps the clock for a mock fixed at 2021-06-25T10:00:00Z.
    let adapter = Adapter::new("https://prebid/bidder").with_clock(|| 1_624_615_200_000);
    let failures = run_json_bidder_test("tests/fixtures/smaato", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
