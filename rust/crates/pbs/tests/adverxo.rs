use pbs::adapters::adverxo::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adverxo", &Adapter::new("https://example.com/auction?id={{.AdUnit}}&auth={{.TokenID}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
