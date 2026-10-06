use pbs::adapters::dianomi::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/dianomi", &Adapter::new("https://prebid-server-aws.dianomi.com/openrtb2/auction"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
