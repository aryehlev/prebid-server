use pbs::adapters::intertech::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/intertech", &Adapter::new("https://test.intertech.com/ssp?pid={{page_id}}&imp={{imp_id}}"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
