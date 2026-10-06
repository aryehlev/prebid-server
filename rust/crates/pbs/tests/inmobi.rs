use pbs::adapters::inmobi::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let adapter = Adapter::new("https://api.w.inmobi.com/showad/openrtb/bidder/prebid").expect("builder");
    let failures = run_json_bidder_test("tests/fixtures/inmobi", &adapter);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
