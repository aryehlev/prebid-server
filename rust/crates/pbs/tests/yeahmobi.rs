use pbs::adapters::yeahmobi::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/yeahmobi", &Adapter::new("https://{{.Host}}/prebid/bid").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
