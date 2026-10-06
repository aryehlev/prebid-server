use pbs::adapters::blasto::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/blasto", &Adapter::new("http://t-us.blasto.ai/bid?rtb_seat_id={{.SourceId}}&secret_key={{.AccountID}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
