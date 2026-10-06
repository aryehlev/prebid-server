use pbs::adapters::revcontent::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/revcontent", &Adapter::new("https://trends.revcontent.com/rtb?userId=1234&apiKey=abcd"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
