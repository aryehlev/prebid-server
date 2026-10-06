use pbs::adapters::smartyads::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let bidder = Adapter::new("http://{{.Host}}.example.com/bid?rtb_seat_id={{.SourceId}}&secret_key={{.AccountID}}").expect("Builder");
    let failures = run_json_bidder_test("tests/fixtures/smartyads", &bidder);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
