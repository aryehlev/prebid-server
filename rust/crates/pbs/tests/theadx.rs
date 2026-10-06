use pbs::adapters::theadx::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/theadx", &Adapter::new("https://ssp.theadx.com/request"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
